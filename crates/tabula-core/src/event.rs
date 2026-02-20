//! Execution output types: events, outcomes, and the execution result.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{CellKey, Value};

/// Monotonically increasing logical timestamp within a batch execution.
pub type LogicalTime = u64;

/// Canonical event identity in M12 E-Trace.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ETraceEventId {
    /// Index of the transaction within the batch (0-based).
    pub tx_index: u32,
    /// Ordinal of this effect within the transaction (0-based).
    pub effect_ordinal_in_tx: u32,
}

/// The kind of state operation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum OpKind {
    /// A read from state.
    Read,
    /// A write to state.
    Write,
}

/// A single execution event for the consistency module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ExecutionEvent {
    /// The cell being accessed.
    pub key: CellKey,
    /// Whether this is a read or write.
    pub op: OpKind,
    /// The value read or written (canonical zero when absent).
    pub value: Value,
    /// Whether the cell is absent (null).
    #[serde(default)]
    pub val_is_null: bool,
    /// Logical time of the operation.
    pub time: LogicalTime,
    /// Index of the transaction within the batch (0-based).
    #[serde(default)]
    pub tx_index: u32,
    /// Ordinal of the effect within the transaction (0-based).
    ///
    /// Canonical identity for M12:
    /// - `tx_index`
    /// - `effect_ordinal_in_tx`
    #[serde(default)]
    pub effect_ordinal_in_tx: u32,
}

impl ExecutionEvent {
    /// Canonical identity of this event in E-Trace.
    pub fn etrace_id(&self) -> ETraceEventId {
        ETraceEventId {
            tx_index: self.tx_index,
            effect_ordinal_in_tx: self.effect_ordinal_in_tx,
        }
    }
}

/// Per-transaction execution outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxOutcome {
    /// Transaction executed successfully.
    Success,
    /// Transaction failed; all its state changes were rolled back.
    Failed {
        /// Human-readable failure reason.
        reason: String,
        /// Execution events produced before the failure (rolled back from state).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        partial_events: Vec<ExecutionEvent>,
        /// Index of the instruction that failed (None for pre-execution failures).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed_instruction: Option<usize>,
    },
}

/// An application-level event emitted during execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmittedEvent {
    /// Topic identifier (application-defined).
    pub topic: Vec<u8>,
    /// Payload data.
    pub data: Vec<Value>,
}

/// Typed consistency check status for command-level contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionConsistencyStatus {
    /// Consistency check passed.
    Passed,
    /// Consistency check failed with reason.
    Failed {
        /// Human-readable failure detail.
        reason: String,
    },
}

/// The output of deterministic batch execution.
///
/// This is the handoff point between Phase A (execution) and Phase B (commitment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Cells read from committed state (not from overlay). Deduplicated.
    /// `None` = cell was absent.
    pub read_set_old: Vec<(CellKey, Option<Value>)>,
    /// Final writes to apply to committed state. Coalesced (last-write-wins).
    /// `None` = delete (write null).
    pub write_set_final: Vec<(CellKey, Option<Value>)>,
    /// Full execution trace for consistency proving.
    pub events: Vec<ExecutionEvent>,
    /// Emitted application events / receipts.
    pub emitted: Vec<EmittedEvent>,
    /// Per-transaction outcomes (success/failure).
    pub tx_outcomes: Vec<TxOutcome>,
}
