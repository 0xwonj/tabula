//! State access trait abstractions: snapshots and static (lookup) tables.

use crate::error::TabulaError;
use crate::{ColId, CommittedCellKey, CommittedKey, PortableValue, RowKey, TableId};

/// Read-only access to the committed state (snapshot).
///
/// The executor uses this to resolve reads that miss the overlay.
pub trait StateView: Send + Sync {
    /// Read a cell from committed state. Returns `None` if absent.
    fn read(&self, key: &CommittedCellKey) -> Result<Option<PortableValue>, TabulaError>;

    /// Iterate all committed cells for one `(table, column)` pair.
    fn column_entries(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(CommittedKey, PortableValue)>, TabulaError>;
}

/// Provides read-only access to static (fixed) tables.
///
/// Used by the LOOKUP instruction for range checks, byte decomposition, enum sets, etc.
pub trait StaticTableProvider: Send + Sync {
    /// Lookup a value in a static table.
    fn lookup(&self, table: TableId, key: RowKey, col: ColId)
    -> Result<PortableValue, TabulaError>;

    /// Check whether a row exists in a static table.
    fn contains(&self, table: TableId, key: RowKey) -> Result<bool, TabulaError>;
}
