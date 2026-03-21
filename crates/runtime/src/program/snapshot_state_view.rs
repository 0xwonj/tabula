//! [`CommittedStateProvider`] backed by a normalized [`State`].
//!
//! Groups state cells by `(table, col)` and returns them in key-sorted order.
//! Used by [`TabulaRuntime::execute()`](crate::TabulaRuntime::execute) to
//! enable `PropertyRead` instructions.
//!
//! No crypto dependencies — reads directly from the artifact-level state cells.

use std::collections::BTreeMap;

use tabula_artifact::{State, StateEntry};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_executor::property::CommittedStateProvider;

type CommittedColumnEntries = Vec<(RowKey, Value, bool)>;
type CommittedColumns = BTreeMap<(TableId, ColId), CommittedColumnEntries>;

pub(crate) struct SnapshotStateView {
    columns: CommittedColumns,
}

impl SnapshotStateView {
    /// Build from normalized state cells.
    pub(crate) fn from_cells(cells: &[StateEntry]) -> Self {
        let mut columns: CommittedColumns = BTreeMap::new();
        for cell in cells {
            if let Some(value) = &cell.value {
                columns
                    .entry((TableId(cell.table), ColId(cell.col)))
                    .or_default()
                    .push((RowKey(cell.row), *value, false));
            }
        }
        for entries in columns.values_mut() {
            entries.sort_by_key(|(k, _, _)| *k);
        }
        Self { columns }
    }

    /// Build from a normalized artifact state value.
    pub(crate) fn from_state(state: &State) -> Self {
        Self::from_cells(&state.cells)
    }
}

impl CommittedStateProvider for SnapshotStateView {
    fn get_column(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(RowKey, Value, bool)>, TabulaError> {
        Ok(self.columns.get(&(table, col)).cloned().unwrap_or_default())
    }
}
