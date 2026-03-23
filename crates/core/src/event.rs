//! Execution output types: events, outcomes, and the execution result.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{CellKey, PortableValue, RowKey};

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

/// A single state-access event (read or write) recorded during execution.
///
/// The tx index is implicit — determined by the event's position within
/// `TxResult::Success { access_trace }` inside `BatchReport.txs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccessEvent {
    /// The cell being accessed.
    pub key: CellKey,
    /// Whether this is a read or write.
    pub op: OpKind,
    /// The value read or written (canonical zero when absent).
    pub value: PortableValue,
    /// Whether the cell is absent (null).
    #[serde(default)]
    pub val_is_null: bool,
    /// Logical time of the operation.
    pub time: LogicalTime,
    /// Ordinal of the effect within the transaction (0-based).
    #[serde(default)]
    pub effect_ordinal_in_tx: u32,
}

/// Structured event recorded during a Precompile instruction execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecompileEvent {
    /// Zero-based index of the transaction within the batch.
    pub tx_index: usize,
    /// Zero-based index of the instruction within the tx body.
    pub instruction_index: usize,
    /// Precompile identifier.
    pub precompile_id: u16,
    /// Input values passed to the precompile.
    pub inputs: Vec<PortableValue>,
    /// Output values returned by the precompile.
    pub outputs: Vec<PortableValue>,
}

/// Canonical result of evaluating a property query against committed state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyQueryResult {
    /// The resolved value.
    pub value: PortableValue,
    /// The key at which the value was found (None if not applicable).
    pub key: Option<RowKey>,
    /// Whether the result is null (no matching row).
    pub is_null: bool,
}

/// Result of a PropertyRead instruction execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyReadResult {
    /// Zero-based index of the instruction within the tx body.
    pub instruction_index: usize,
    /// The resolved value.
    pub value: PortableValue,
    /// The key at which the value was found (None if not applicable).
    pub key: Option<RowKey>,
    /// Whether the result is null (no matching row).
    pub is_null: bool,
}

/// Per-transaction execution result.
///
/// Carries per-tx data: on success, the access trace and emitted events;
/// on failure, diagnostic information about what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxResult {
    /// Transaction executed successfully.
    Success {
        /// Application events emitted during this transaction.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        emitted: Vec<EmittedEvent>,
        /// State-access events recorded during this transaction.
        access_trace: Vec<AccessEvent>,
        /// Precompile events recorded during this transaction.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        precompile_events: Vec<PrecompileEvent>,
        /// Property read results recorded during this transaction.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        property_reads: Vec<PropertyReadResult>,
    },
    /// Transaction failed; all its state changes were rolled back.
    Failed {
        /// Human-readable failure reason.
        reason: String,
        /// Access events produced before the failure (rolled back from state).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        partial_events: Vec<AccessEvent>,
        /// Index of the instruction that failed (None for pre-execution failures).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed_instruction: Option<usize>,
    },
}

impl TxResult {
    /// Create a successful result with only access trace and emitted events.
    pub fn success(access_trace: Vec<AccessEvent>, emitted: Vec<EmittedEvent>) -> Self {
        Self::Success {
            emitted,
            access_trace,
            precompile_events: vec![],
            property_reads: vec![],
        }
    }

    /// Whether this transaction succeeded.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Access trace for successful transactions, empty slice for failed ones.
    pub fn access_trace(&self) -> &[AccessEvent] {
        match self {
            Self::Success { access_trace, .. } => access_trace,
            Self::Failed { .. } => &[],
        }
    }
}

/// An application-level event emitted during execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmittedEvent {
    /// Topic identifier (application-defined).
    pub topic: Vec<u8>,
    /// Payload data.
    pub data: Vec<PortableValue>,
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
/// This is a public reporting and boundary projection of execution.
///
/// Internally, the runtime is moving toward a canonical typed execution journal
/// as the semantic source of truth for proving. `BatchReport` remains the
/// stable public view that exposes base-state reads, final writes, and
/// per-transaction outcomes in a portable form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReport {
    /// Cells read from committed state (not from overlay). Deduplicated.
    /// `None` = cell was absent.
    pub read_set_old: Vec<(CellKey, Option<PortableValue>)>,
    /// Final writes to apply to committed state. Coalesced (last-write-wins).
    /// `None` = delete (write null).
    pub write_set_final: Vec<(CellKey, Option<PortableValue>)>,
    /// Per-transaction results, in batch order.
    pub txs: Vec<TxResult>,
}

impl BatchReport {
    /// Iterate access events from all successful transactions, preserving order.
    pub fn successful_events(&self) -> impl Iterator<Item = &AccessEvent> + '_ {
        self.txs
            .iter()
            .filter_map(|tx| match tx {
                TxResult::Success { access_trace, .. } => Some(access_trace.iter()),
                TxResult::Failed { .. } => None,
            })
            .flatten()
    }

    /// Iterate access events from all successful transactions with tx index.
    pub fn successful_events_with_tx(&self) -> impl Iterator<Item = (u32, &AccessEvent)> + '_ {
        self.txs
            .iter()
            .enumerate()
            .filter_map(|(i, tx)| match tx {
                TxResult::Success { access_trace, .. } => {
                    Some(access_trace.iter().map(move |e| (i as u32, e)))
                }
                TxResult::Failed { .. } => None,
            })
            .flatten()
    }

    /// Iterate emitted events from all successful transactions, preserving order.
    pub fn successful_emitted(&self) -> impl Iterator<Item = &EmittedEvent> + '_ {
        self.txs
            .iter()
            .filter_map(|tx| match tx {
                TxResult::Success { emitted, .. } => Some(emitted.iter()),
                TxResult::Failed { .. } => None,
            })
            .flatten()
    }
}
