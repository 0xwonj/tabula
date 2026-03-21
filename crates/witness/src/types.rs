//! Logical shared input types for runtime-owned proof preparation.

use tabula_core::{CellKey, LogicalTime, PropertyQueryResult, RowKey, Value};
use tabula_ir::PropertyQuery;

/// Logical committed-state entry for one row of a committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedEntry {
    /// Row key.
    pub row: RowKey,
    /// Logical cell value.
    pub value: Value,
    /// Whether the entry is absent in committed state.
    pub is_null: bool,
}

/// Base-state seed for one committed cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitCell {
    /// The cell address.
    pub key: CellKey,
    /// The logical cell value.
    pub value: Value,
    /// Whether the cell is absent in committed state.
    pub is_null: bool,
}

/// Execution-time access event grouped per column for proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessEvent {
    /// The cell address.
    pub key: CellKey,
    /// Logical time of this access.
    pub time: LogicalTime,
    /// Whether this event is a write.
    pub is_write: bool,
    /// The logical cell value observed or written by the executor.
    pub value: Value,
    /// Whether the value is null.
    pub is_null: bool,
    /// Transaction index within the batch.
    pub tx_index: u32,
    /// Effect ordinal within the transaction.
    pub effect_ordinal_in_tx: u32,
}

/// Final coalesced write for one row of a committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnWrite {
    /// Target row key.
    pub row: RowKey,
    /// Final logical value. `None` means delete / write null.
    pub value: Option<Value>,
}

/// Logical property-read claim extracted from execution for proof preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyReadClaim {
    /// Original structural query.
    pub query: PropertyQuery,
    /// Execution result claimed for this query.
    pub result: PropertyQueryResult,
}
