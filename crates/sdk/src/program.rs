use tabula_core::RowKey;
use tabula_ir as ir;

use crate::Sdk;
use crate::artifact::Artifact;
use crate::batch::TransactionBatch;
use crate::context::Context;
use crate::error::SdkError;
#[cfg(feature = "execute")]
use crate::runner::Runner;
use crate::schema::{ContextFieldHandle, QueryHandle, Schema, TableHandle, TxHandle};
use crate::state::State;
use crate::value::{EncodeArgs, EncodeValue};
#[cfg(feature = "verify")]
use crate::verifier::Verifier;

/// User-facing opened program handle.
#[derive(Clone)]
pub struct Program {
    sdk: Sdk,
    artifact: Artifact,
}

impl Program {
    pub(crate) fn new(sdk: Sdk, artifact: Artifact) -> Self {
        Self { sdk, artifact }
    }

    pub(crate) fn sdk(&self) -> &Sdk {
        &self.sdk
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub fn schema(&self) -> &Schema {
        self.artifact.schema()
    }

    pub fn tx(&self, symbol: &str) -> Result<TxHandle, SdkError> {
        self.schema().tx(symbol)
    }

    pub fn query(&self, symbol: &str) -> Result<QueryHandle, SdkError> {
        self.schema().query(symbol)
    }

    pub fn table(&self, symbol: &str) -> Result<TableHandle, SdkError> {
        self.schema().table(symbol)
    }

    pub fn context_field(&self, symbol: &str) -> Result<ContextFieldHandle, SdkError> {
        self.schema().context_field(symbol)
    }

    pub fn state(&self) -> StateBuilder {
        StateBuilder::new(self.clone())
    }

    pub fn context(&self) -> ContextBuilder {
        ContextBuilder::new(self.clone())
    }

    pub fn batch(&self) -> TransactionBatchBuilder {
        TransactionBatchBuilder::new(self.clone())
    }

    #[cfg(feature = "execute")]
    pub fn runner(&self) -> Runner {
        Runner::new(self.clone())
    }

    #[cfg(feature = "verify")]
    pub fn verifier(&self) -> Verifier {
        Verifier::new(self.clone())
    }
}

/// Symbol-first committed state builder.
#[derive(Clone)]
pub struct StateBuilder {
    program: Program,
    state: State,
}

impl StateBuilder {
    fn new(program: Program) -> Self {
        let state = State::from_raw(tabula_runtime::StateSnapshot::empty(
            program.artifact.registered().program(),
        ));
        Self { program, state }
    }

    pub fn set<V: EncodeValue>(
        mut self,
        table_symbol: &str,
        row: u64,
        field_symbol: &str,
        value: V,
    ) -> Result<Self, SdkError> {
        let table = self.program.table(table_symbol)?;
        let field = table.field(field_symbol)?;
        let portable = value.encode_for(field.ty())?;
        self.state
            .0
            .insert(
                self.program.artifact.registered().program(),
                table.id(),
                RowKey(row),
                field.id(),
                portable,
            )
            .map_err(SdkError::from)?;
        Ok(self)
    }

    pub fn build(self) -> State {
        self.state
    }
}

/// Symbol-first public context builder.
#[derive(Clone)]
pub struct ContextBuilder {
    program: Program,
    context: Context,
}

impl ContextBuilder {
    fn new(program: Program) -> Self {
        Self {
            program,
            context: Context::default(),
        }
    }

    pub fn set<V: EncodeValue>(mut self, symbol: &str, value: V) -> Result<Self, SdkError> {
        let field = self.program.context_field(symbol)?;
        let portable = value.encode_for(field.ty())?;
        self.context.0.fields.insert(field.id(), portable);
        Ok(self)
    }

    pub fn build(self) -> Context {
        self.context
    }
}

/// Symbol-first transaction batch builder.
#[derive(Clone)]
pub struct TransactionBatchBuilder {
    program: Program,
    batch: TransactionBatch,
}

impl TransactionBatchBuilder {
    fn new(program: Program) -> Self {
        Self {
            program,
            batch: TransactionBatch::default(),
        }
    }

    pub fn call<A>(mut self, symbol: &str, params: A) -> Result<Self, SdkError>
    where
        A: EncodeArgs,
    {
        let tx = self.program.tx(symbol)?;
        let params = params.encode_args(tx.params())?;
        self.batch.0.calls.push(ir::EntryCall {
            entry_id: tx.id(),
            params,
        });
        Ok(self)
    }

    pub fn build(self) -> TransactionBatch {
        self.batch
    }
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("artifact_digest", &self.artifact.digest())
            .finish_non_exhaustive()
    }
}
