//! [`CommittedStateProvider`] backed by a normalized [`StateFile`].
//!
//! Groups state cells by `(table, col)` and returns them in key-sorted order.
//! Used by [`TabulaRuntime::execute()`](crate::TabulaRuntime::execute) to
//! enable `PropertyRead` instructions.
//!
//! No crypto dependencies — reads directly from the artifact-level state cells.

use std::collections::BTreeMap;

use tabula_artifact::StateCell;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_executor::property::CommittedStateProvider;

pub(crate) struct StateFileCommittedState {
    columns: BTreeMap<(TableId, ColId), Vec<(RowKey, Value, bool)>>,
}

impl StateFileCommittedState {
    /// Build from normalized state cells.
    pub(crate) fn from_cells(cells: &[StateCell]) -> Self {
        let mut columns: BTreeMap<(TableId, ColId), Vec<(RowKey, Value, bool)>> = BTreeMap::new();
        for cell in cells {
            if let Some(value) = &cell.value {
                columns
                    .entry((TableId(cell.table), ColId(cell.col)))
                    .or_default()
                    .push((RowKey(cell.row), value.clone(), false));
            }
        }
        for entries in columns.values_mut() {
            entries.sort_by_key(|(k, _, _)| *k);
        }
        Self { columns }
    }
}

impl CommittedStateProvider for StateFileCommittedState {
    fn get_column(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(RowKey, Value, bool)>, TabulaError> {
        Ok(self.columns.get(&(table, col)).cloned().unwrap_or_default())
    }
}
