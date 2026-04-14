//! Execution and proving runner for an opened program.

use tabula_core::PortableValue;
use tabula_executor as exec;

use super::Program;
use crate::error::SdkError;
#[cfg(feature = "prove")]
use crate::types::Proof;
use crate::types::{Context, State, TransactionBatch};
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
    /// Zero-based index of this transaction within the batch.
    pub fn tx_index(&self) -> u32 {
        self.tx_index
    }

    /// Entry identifier of the executed transaction.
    pub fn entry_id(&self) -> tabula_ir::EntryId {
        self.entry_id
    }

    /// Whether the transaction completed successfully.
    pub fn success(&self) -> bool {
        self.success
    }

    /// Human-readable failure reason, if the transaction failed.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Index of the failing IR operation, if the transaction failed.
    pub fn failed_op_index(&self) -> Option<usize> {
        self.failed_op_index
    }

    /// Number of state cell writes produced by this transaction.
    pub fn state_effect_count(&self) -> usize {
        self.state_effect_count
    }

    /// Number of events emitted by this transaction.
    pub fn event_effect_count(&self) -> usize {
        self.event_effect_count
    }

    /// Number of native capability invocations made by this transaction.
    pub fn capability_effect_count(&self) -> usize {
        self.capability_effect_count
    }

    /// Number of static relation lookups performed by this transaction.
    pub fn relation_effect_count(&self) -> usize {
        self.relation_effect_count
    }
}

/// Runtime-owned execution result surfaced through the SDK.
#[derive(Debug, Clone)]
pub struct ExecutionReceipt {
    #[cfg(feature = "prove")]
    pub(crate) program_digest: String,
    pub(crate) state_before: State,
    pub(crate) state_after: State,
    pub(crate) inner: tabula_runtime::ExecutionReceipt,
}

impl ExecutionReceipt {
    pub(crate) fn from_runtime(
        #[cfg(feature = "prove")] program_digest: String,
        state_before: State,
        state_after: State,
        inner: tabula_runtime::ExecutionReceipt,
    ) -> Self {
        Self {
            #[cfg(feature = "prove")]
            program_digest,
            state_before,
            state_after,
            inner,
        }
    }

    /// The state snapshot that was provided as input to the batch.
    pub fn state_before(&self) -> State {
        self.state_before.clone()
    }

    /// The state snapshot resulting from applying all successful transactions.
    pub fn state_after(&self) -> State {
        self.state_after.clone()
    }

    /// The transaction batch that was executed.
    pub fn batch(&self) -> TransactionBatch {
        TransactionBatch::from_raw(self.inner.batch.clone())
    }

    /// The public context that was supplied to the batch.
    pub fn context(&self) -> Context {
        Context::from_raw(self.inner.context.clone())
    }

    /// Number of distinct state cells read during execution.
    pub fn read_count(&self) -> usize {
        self.inner.journal.state_summary.read_set_old.len()
    }

    /// Number of distinct state cells written during execution.
    pub fn write_count(&self) -> usize {
        self.inner.journal.state_summary.write_set_final.len()
    }

    /// Per-transaction outcome summaries in batch order.
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

    /// Borrow all portable return values in declaration order.
    pub fn returns(&self) -> &[PortableValue] {
        &self.returns
    }

    /// Number of return values.
    pub fn len(&self) -> usize {
        self.returns.len()
    }

    /// Whether the result carries zero return values.
    pub fn is_empty(&self) -> bool {
        self.returns.is_empty()
    }

    /// Decode all return values as `T`.
    pub fn decode_all<T: DecodeValue>(&self) -> Result<Vec<T>, SdkError> {
        self.returns.iter().map(T::decode_from).collect()
    }

    /// Decode a single return value; returns an error if the count is not exactly one.
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

    /// Pre-warm the runtime cache so the first call has no cold-start penalty.
    pub fn warm(&self) -> Result<(), SdkError> {
        let _ = self.runtime()?;
        Ok(())
    }

    /// Execute a transaction batch and return a receipt with per-tx outcomes.
    pub fn execute(
        &self,
        state: &State,
        batch: &TransactionBatch,
        context: &Context,
    ) -> Result<ExecutionReceipt, SdkError> {
        let runtime = self.runtime()?;
        let snapshot = runtime.materialize_logical_state(
            state
                .cells_raw()
                .iter()
                .cloned()
                .map(|cell| (cell.table, cell.key, cell.field, cell.value)),
        )?;
        let receipt = runtime.execute_batch_receipt(&snapshot, batch.as_raw(), context.as_raw())?;
        let state_after = State::from_cells(
            runtime
                .project_logical_state(&receipt.state_after)?
                .into_iter()
                .map(
                    |(table, key, field, value)| crate::types::LogicalStateCell {
                        table,
                        key,
                        field,
                        value,
                    },
                )
                .collect(),
        );
        Ok(ExecutionReceipt::from_runtime(
            #[cfg(feature = "prove")]
            self.program.artifact().digest().to_string(),
            state.clone(),
            state_after,
            receipt,
        ))
    }

    /// Execute a single query and return its portable return values.
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
        let snapshot = runtime.materialize_logical_state(
            state
                .cells_raw()
                .iter()
                .cloned()
                .map(|cell| (cell.table, cell.key, cell.field, cell.value)),
        )?;
        let result = runtime.execute_query(&snapshot, query.id(), &params, context.as_raw())?;
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

    /// Execute a single query by source symbol and return its portable return values.
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

    /// Generate a STARK proof from a previously obtained [`ExecutionReceipt`].
    #[cfg(feature = "prove")]
    pub fn prove(&self, receipt: &ExecutionReceipt) -> Result<Proof, SdkError> {
        if receipt.program_digest != self.program.artifact().digest() {
            return Err(SdkError::ExecutionProgramMismatch);
        }
        let runtime = self.runtime()?;
        let result = runtime.prove(&tabula_runtime::ProveInput {
            snapshot: &receipt.inner.snapshot,
            batch: &receipt.inner.batch,
            context: &receipt.inner.context,
            executed: &receipt.inner.journal,
        })?;
        Ok(Proof::from_prove_result(result))
    }

    /// Execute a batch and immediately generate a STARK proof in one call.
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

    /// Look up a transaction entry by source symbol.
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
