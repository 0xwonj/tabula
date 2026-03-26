use tabula_core::PortableValue;
use tabula_executor as exec;

use crate::batch::TransactionBatch;
use crate::context::Context;
use crate::error::SdkError;
use crate::program::Program;
#[cfg(feature = "prove")]
use crate::proof::Proof;
use crate::state::State;
use crate::value::{DecodeValue, EncodeArgs};
use crate::{QueryHandle, TxHandle};

/// One portable batch outcome summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOutcomeSummary {
    tx_index: u32,
    entry_id: tabula_ir::EntryId,
    success: bool,
    reason: Option<String>,
    failed_op_index: Option<usize>,
    state_effect_count: usize,
    event_effect_count: usize,
    capability_effect_count: usize,
    relation_effect_count: usize,
}

impl TxOutcomeSummary {
    pub fn tx_index(&self) -> u32 {
        self.tx_index
    }

    pub fn entry_id(&self) -> tabula_ir::EntryId {
        self.entry_id
    }

    pub fn success(&self) -> bool {
        self.success
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn failed_op_index(&self) -> Option<usize> {
        self.failed_op_index
    }

    pub fn state_effect_count(&self) -> usize {
        self.state_effect_count
    }

    pub fn event_effect_count(&self) -> usize {
        self.event_effect_count
    }

    pub fn capability_effect_count(&self) -> usize {
        self.capability_effect_count
    }

    pub fn relation_effect_count(&self) -> usize {
        self.relation_effect_count
    }
}

/// Runtime-owned execution result surfaced through the SDK.
#[derive(Debug, Clone)]
pub struct ExecutionReceipt {
    #[cfg(feature = "prove")]
    pub(crate) program_digest: String,
    pub(crate) inner: tabula_runtime::ExecutionReceipt,
}

impl ExecutionReceipt {
    pub(crate) fn from_runtime(
        #[cfg(feature = "prove")] program_digest: String,
        inner: tabula_runtime::ExecutionReceipt,
    ) -> Self {
        Self {
            #[cfg(feature = "prove")]
            program_digest,
            inner,
        }
    }

    pub fn state_before(&self) -> State {
        State::from_raw(self.inner.snapshot.clone())
    }

    pub fn state_after(&self) -> State {
        State::from_raw(self.inner.state_after.clone())
    }

    pub fn batch(&self) -> TransactionBatch {
        TransactionBatch::from_raw(self.inner.batch.clone())
    }

    pub fn context(&self) -> Context {
        Context::from_raw(self.inner.context.clone())
    }

    pub fn read_count(&self) -> usize {
        self.inner.journal.state_summary.read_set_old.len()
    }

    pub fn write_count(&self) -> usize {
        self.inner.journal.state_summary.write_set_final.len()
    }

    pub fn outcomes(&self) -> Vec<TxOutcomeSummary> {
        self.inner
            .journal
            .txs
            .iter()
            .map(|tx| match tx {
                exec::TxExecutionOutcome::Success(success) => TxOutcomeSummary {
                    tx_index: success.tx_index,
                    entry_id: success.entry_id,
                    success: true,
                    reason: None,
                    failed_op_index: None,
                    state_effect_count: success.state_effects.len(),
                    event_effect_count: success.event_effects.len(),
                    capability_effect_count: success.capability_effects.len(),
                    relation_effect_count: success.relation_effects.len(),
                },
                exec::TxExecutionOutcome::Failed(failure) => TxOutcomeSummary {
                    tx_index: failure.tx_index,
                    entry_id: failure.entry_id,
                    success: false,
                    reason: Some(failure.reason.clone()),
                    failed_op_index: failure.failed_op_index,
                    state_effect_count: 0,
                    event_effect_count: 0,
                    capability_effect_count: 0,
                    relation_effect_count: 0,
                },
            })
            .collect()
    }
}

/// Portable query result returned by the SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    returns: Vec<PortableValue>,
}

impl QueryResult {
    pub(crate) fn new(returns: Vec<PortableValue>) -> Self {
        Self { returns }
    }

    pub fn len(&self) -> usize {
        self.returns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.returns.is_empty()
    }

    pub fn decode_all<T: DecodeValue>(&self) -> Result<Vec<T>, SdkError> {
        self.returns.iter().map(T::decode_from).collect()
    }

    pub fn decode_one<T: DecodeValue>(&self) -> Result<T, SdkError> {
        let [value] = self.returns.as_slice() else {
            return Err(SdkError::ValueDecoding {
                detail: format!(
                    "query returned {} values but exactly 1 was expected",
                    self.returns.len()
                ),
            });
        };
        T::decode_from(value)
    }
}

/// Prepared execution/proving handle for one `(artifact, environment)` pair.
#[derive(Clone)]
pub struct Runner {
    program: Program,
}

impl Runner {
    pub(crate) fn new(program: Program) -> Self {
        Self { program }
    }

    pub fn warm(&self) -> Result<(), SdkError> {
        let _ = self.runtime()?;
        Ok(())
    }

    pub fn execute(
        &self,
        state: &State,
        batch: &TransactionBatch,
        context: &Context,
    ) -> Result<ExecutionReceipt, SdkError> {
        let receipt = self.runtime()?.execute_batch_receipt(
            state.as_raw(),
            batch.as_raw(),
            context.as_raw(),
        )?;
        Ok(ExecutionReceipt::from_runtime(
            #[cfg(feature = "prove")]
            self.program.artifact().digest().to_string(),
            receipt,
        ))
    }

    pub fn query<A>(
        &self,
        state: &State,
        query: &QueryHandle,
        params: A,
        context: &Context,
    ) -> Result<QueryResult, SdkError>
    where
        A: EncodeArgs,
    {
        let params = params.encode_args(query.params())?;
        let runtime = self.runtime()?;
        let result =
            runtime.execute_query(state.as_raw(), query.id(), &params, context.as_raw())?;
        let returns = result
            .returns
            .iter()
            .map(|value| runtime.type_runtimes().encode_typed(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| {
                SdkError::Runtime(tabula_runtime::RuntimeError::ValidationFailed {
                    detail: source.to_string(),
                })
            })?;
        Ok(QueryResult::new(returns))
    }

    pub fn query_symbol<A>(
        &self,
        state: &State,
        symbol: &str,
        params: A,
        context: &Context,
    ) -> Result<QueryResult, SdkError>
    where
        A: EncodeArgs,
    {
        let query = self.program.query(symbol)?;
        self.query(state, &query, params, context)
    }

    #[cfg(feature = "prove")]
    pub fn prove(&self, receipt: &ExecutionReceipt) -> Result<Proof, SdkError> {
        if receipt.program_digest != self.program.artifact().digest() {
            return Err(SdkError::ExecutionProgramMismatch);
        }
        let result = self.runtime()?.prove(&tabula_runtime::ProveInput {
            snapshot: &receipt.inner.snapshot,
            batch: &receipt.inner.batch,
            context: &receipt.inner.context,
            executed: &receipt.inner.journal,
        })?;
        Ok(Proof::from_prove_result(result))
    }

    #[cfg(feature = "prove")]
    pub fn execute_and_prove(
        &self,
        state: &State,
        batch: &TransactionBatch,
        context: &Context,
    ) -> Result<(ExecutionReceipt, Proof), SdkError> {
        let receipt = self.execute(state, batch, context)?;
        let proof = self.prove(&receipt)?;
        Ok((receipt, proof))
    }

    pub fn tx(&self, symbol: &str) -> Result<TxHandle, SdkError> {
        self.program.tx(symbol)
    }

    fn runtime(&self) -> Result<std::sync::Arc<tabula_runtime::TabulaRuntime>, SdkError> {
        self.program.sdk().prepare_runtime(self.program.artifact())
    }
}

impl std::fmt::Debug for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("artifact_digest", &self.program.artifact().digest())
            .finish_non_exhaustive()
    }
}
