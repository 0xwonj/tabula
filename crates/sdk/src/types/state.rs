use serde::{Deserialize, Serialize};
use tabula_core::{CellKey, PortableValue, RowKey};
use tabula_runtime::StateSnapshot;

/// Portable committed state snapshot on the SDK happy path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct State(pub(crate) StateSnapshot);

impl State {
    pub(crate) fn from_raw(raw: StateSnapshot) -> Self {
        Self(raw)
    }

    pub(crate) fn as_raw(&self) -> &StateSnapshot {
        &self.0
    }

    /// Iterate over all committed (key, value) cell pairs in the snapshot.
    pub fn cells(&self) -> impl Iterator<Item = (&CellKey, &PortableValue)> {
        self.0.cells()
    }

    /// Remove a cell from the snapshot by (table, row, field).
    pub fn remove(&mut self, table: tabula_ir::TableId, row: RowKey, field: tabula_ir::FieldId) {
        self.0.remove(table, row, field);
    }
}
