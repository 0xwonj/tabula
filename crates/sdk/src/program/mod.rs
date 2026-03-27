//! User-facing program handle and builder types.
//!
//! This module provides the primary entry point for interacting with a deployed
//! Tabula program: looking up schema handles, constructing state/context/batch
//! inputs, and obtaining execution or verification runners.

mod artifact;
#[cfg(feature = "execute")]
mod runner;
mod schema;
#[cfg(feature = "verify")]
mod verifier;

pub use artifact::Artifact;
#[cfg(feature = "execute")]
pub use runner::{ExecutionReceipt, QueryResult, Runner, TxOutcomeSummary};
pub use schema::{
    ContextFieldHandle, FieldHandle, ParameterHandle, QueryHandle, Schema, TableHandle, TxHandle,
};
#[cfg(feature = "verify")]
pub use verifier::Verifier;

use tabula_core::RowKey;
use tabula_ir as ir;

use crate::Sdk;
use crate::error::SdkError;
use crate::types::{Context, State, TransactionBatch};
use crate::value::{EncodeArgs, EncodeValue};

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

    /// Return the compiled artifact backing this program.
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    /// Return the program schema (tables, transactions, queries, context fields).
    pub fn schema(&self) -> &Schema {
        self.artifact.schema()
    }

    /// Look up a transaction entry by source symbol.
    pub fn tx(&self, symbol: &str) -> Result<TxHandle, SdkError> {
        self.schema().tx(symbol)
    }

    /// Look up a query entry by source symbol.
    pub fn query(&self, symbol: &str) -> Result<QueryHandle, SdkError> {
        self.schema().query(symbol)
    }

    /// Look up a state table by source symbol.
    pub fn table(&self, symbol: &str) -> Result<TableHandle, SdkError> {
        self.schema().table(symbol)
    }

    /// Look up a public context field by source symbol.
    pub fn context_field(&self, symbol: &str) -> Result<ContextFieldHandle, SdkError> {
        self.schema().context_field(symbol)
    }

    /// Start building a state snapshot using symbol-based field names.
    pub fn state(&self) -> StateBuilder {
        StateBuilder::new(self.clone())
    }

    /// Start building a public context input using symbol-based field names.
    pub fn context(&self) -> ContextBuilder {
        ContextBuilder::new(self.clone())
    }

    /// Start building a transaction batch using symbol-based entry names.
    pub fn batch(&self) -> TransactionBatchBuilder {
        TransactionBatchBuilder::new(self.clone())
    }

    /// Create an execution (and optionally proving) runner for this program.
    #[cfg(feature = "execute")]
    pub fn runner(&self) -> Runner {
        Runner::new(self.clone())
    }

    /// Create a proof verifier for this program.
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

    /// Write a field value into the state snapshot by table/field symbols.
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

    /// Finalize the builder and return the constructed [`State`].
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

    /// Set a context field value by source symbol.
    pub fn set<V: EncodeValue>(mut self, symbol: &str, value: V) -> Result<Self, SdkError> {
        let field = self.program.context_field(symbol)?;
        let portable = value.encode_for(field.ty())?;
        self.context.0.fields.insert(field.id(), portable);
        Ok(self)
    }

    /// Finalize the builder and return the constructed [`Context`].
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

    /// Append a transaction call by source symbol with encoded parameters.
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

    /// Finalize the builder and return the constructed [`TransactionBatch`].
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
