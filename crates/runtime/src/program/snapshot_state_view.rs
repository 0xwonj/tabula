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
use tabula_core::{ColId, TableId};
use tabula_executor::property::CommittedStateProvider;
use tabula_types::{TypeRuntimeRegistry, TypedColumnEntry};

type CommittedColumnEntries = Vec<TypedColumnEntry>;
type CommittedColumns = BTreeMap<(TableId, ColId), CommittedColumnEntries>;

pub(crate) struct SnapshotStateView {
    columns: CommittedColumns,
}

impl SnapshotStateView {
    /// Build from normalized state cells.
    pub(crate) fn from_cells(cells: &[StateEntry], type_runtimes: &TypeRuntimeRegistry) -> Self {
        let mut columns: CommittedColumns = BTreeMap::new();
        for cell in cells {
            if let Some(value) = &cell.value {
                let typed = type_runtimes
                    .decode_portable(value)
                    .expect("normalized state cell must decode");
                columns
                    .entry((TableId(cell.table), ColId(cell.col)))
                    .or_default()
                    .push(TypedColumnEntry {
                        row_key: tabula_core::RowKey(cell.row),
                        value: typed,
                        is_null: false,
                    });
            }
        }
        for entries in columns.values_mut() {
            entries.sort_by_key(|entry| entry.row_key);
        }
        Self { columns }
    }

    /// Build from a normalized artifact state value.
    pub(crate) fn from_state(state: &State, type_runtimes: &TypeRuntimeRegistry) -> Self {
        Self::from_cells(&state.cells, type_runtimes)
    }
}

impl CommittedStateProvider for SnapshotStateView {
    fn get_column(&self, table: TableId, col: ColId) -> Result<Vec<TypedColumnEntry>, TabulaError> {
        Ok(self.columns.get(&(table, col)).cloned().unwrap_or_default())
    }
}
