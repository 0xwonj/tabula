//! Execution journal: per-batch and per-transaction outcomes with typed effects.

use tabula_core::{CommittedCellKey, TypeId};
use tabula_ir as ir;
use tabula_types::{
    RelationEffect, StatePropertyEffect, TypedEventEffect, TypedStateEffect, TypedValue,
};

/// The result of executing a single query entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExecutionResult {
    /// Return values produced by the query.
    pub returns: Vec<TypedValue>,
    /// Aggregate state read/write summary for the query.
    pub state_summary: ExecutionStateSummary,
    /// Individual state cell effects observed during execution.
    pub state_effects: Vec<TypedStateEffect>,
    /// State property reads performed during execution.
    pub property_effects: Vec<StatePropertyEffect>,
    /// Static relation lookups performed during execution.
    pub relation_effects: Vec<RelationEffect>,
    /// Native capability invocations performed during execution.
    pub capability_effects: Vec<CapabilityEffect>,
    /// Events emitted during execution.
    pub event_effects: Vec<TypedEventEffect>,
}

/// Journal of all transaction outcomes for one executed batch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionJournal {
    /// Aggregate state read/write summary for the entire batch.
    pub state_summary: ExecutionStateSummary,
    /// Per-transaction outcomes in batch order.
    pub txs: Vec<TxExecutionOutcome>,
}

impl ExecutionJournal {
    /// Iterate over only the successful transaction outcomes.
    pub fn successful_txs(&self) -> impl Iterator<Item = &SuccessfulTxExecution> + '_ {
        self.txs.iter().filter_map(TxExecutionOutcome::success)
    }
}

/// Outcome of a single transaction execution: either success or failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxExecutionOutcome {
    /// The transaction completed without error.
    Success(SuccessfulTxExecution),
    /// The transaction aborted (e.g., failed assertion).
    Failed(FailedTxExecution),
}

impl TxExecutionOutcome {
    /// Return the success case if applicable.
    pub fn success(&self) -> Option<&SuccessfulTxExecution> {
        match self {
            Self::Success(success) => Some(success),
            Self::Failed(_) => None,
        }
    }

    /// Return the failure case if applicable.
    pub fn failure(&self) -> Option<&FailedTxExecution> {
        match self {
            Self::Success(_) => None,
            Self::Failed(failure) => Some(failure),
        }
    }
}

/// All effects produced by a successfully completed transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessfulTxExecution {
    /// Zero-based index of this transaction within the batch.
    pub tx_index: u32,
    /// Entry identifier for the transaction.
    pub entry_id: ir::EntryId,
    /// All state cell read/write/delete effects in execution order.
    pub state_effects: Vec<TypedStateEffect>,
    /// State property reads performed during the transaction.
    pub property_effects: Vec<StatePropertyEffect>,
    /// Static relation lookups performed during the transaction.
    pub relation_effects: Vec<RelationEffect>,
    /// Native capability invocations performed during the transaction.
    pub capability_effects: Vec<CapabilityEffect>,
    /// Events emitted during the transaction.
    pub event_effects: Vec<TypedEventEffect>,
}

/// Failure record for a transaction that aborted before completing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedTxExecution {
    /// Zero-based index of this transaction within the batch.
    pub tx_index: u32,
    /// Entry identifier for the transaction.
    pub entry_id: ir::EntryId,
    /// Human-readable reason for the failure.
    pub reason: String,
    /// Index of the IR operation that triggered the failure, if known.
    pub failed_op_index: Option<usize>,
}

/// Aggregate state read/write summary for a batch or query.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionStateSummary {
    /// Snapshot of every distinct cell value read (old value before any writes).
    pub read_set_old: Vec<TypedStateSnapshot>,
    /// Final value of every distinct cell that was written.
    pub write_set_final: Vec<TypedStateWrite>,
}

/// A snapshot of a single state cell value at the start of a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateSnapshot {
    /// The cell key (table, column, row).
    pub key: CommittedCellKey,
    /// Type of the cell's value.
    pub type_id: TypeId,
    /// The value at the start of the batch (`None` if absent).
    pub value: Option<TypedValue>,
}

/// The final written value for a single state cell after batch execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateWrite {
    /// The cell key (table, column, row).
    pub key: CommittedCellKey,
    /// Type of the cell's value.
    pub type_id: TypeId,
    /// The final value after all writes (`None` if deleted).
    pub value: Option<TypedValue>,
}

/// A single native capability invocation within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEffect {
    /// Target capability.
    pub capability: ir::CapabilityId,
    /// Input values supplied to the capability.
    pub inputs: Vec<TypedValue>,
    /// Output values returned by the capability.
    pub outputs: Vec<TypedValue>,
    /// Index of the IR operation that produced this effect.
    pub op_index: usize,
    /// Ordinal of this effect among all effects within the enclosing entry execution.
    pub effect_ordinal_in_entry: u32,
}
