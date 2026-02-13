//! Execution output types: events, outcomes, and the execution result.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::types::{CellKey, Value};

/// Monotonically increasing logical timestamp within a batch execution.
pub type LogicalTime = u64;

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
    /// The value read or written.
    pub value: Value,
    /// Logical time of the operation.
    pub time: LogicalTime,
    /// Index of the transaction within the batch (0-based).
    #[serde(default)]
    pub tx_index: u32,
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

/// The output of deterministic batch execution.
///
/// This is the handoff point between Phase A (execution) and Phase B (commitment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Cells read from committed state (not from overlay). Deduplicated.
    pub read_set_old: Vec<(CellKey, Value)>,
    /// Final writes to apply to committed state. Coalesced (last-write-wins).
    pub write_set_final: Vec<(CellKey, Value)>,
    /// Full execution trace for consistency proving.
    pub events: Vec<ExecutionEvent>,
    /// Emitted application events / receipts.
    pub emitted: Vec<EmittedEvent>,
    /// Per-transaction outcomes (success/failure).
    pub tx_outcomes: Vec<TxOutcome>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ColId, RowKey, TableId};

    #[test]
    fn test_execution_event_borsh_round_trip() {
        let event = ExecutionEvent {
            key: CellKey {
                table: TableId(1),
                col: ColId(0),
                row: RowKey(0),
            },
            op: OpKind::Read,
            value: Value::U64(100),
            time: 1,
            tx_index: 0,
        };
        let bytes = borsh::to_vec(&event).unwrap();
        let decoded: ExecutionEvent = borsh::from_slice(&bytes).unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn test_execution_result_construction() {
        let result = ExecutionResult {
            read_set_old: vec![],
            write_set_final: vec![],
            events: vec![],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        assert_eq!(result.tx_outcomes.len(), 1);
    }
}
