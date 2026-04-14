use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

/// One logical state cell authored through the SDK surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LogicalStateCell {
    /// Table identifier.
    pub table: tabula_ir::TableId,
    /// Logical key tuple in declaration order.
    pub key: Vec<PortableValue>,
    /// Field identifier within the table.
    pub field: tabula_ir::FieldId,
    /// Portable field value.
    pub value: PortableValue,
}

/// Portable logical user-state input on the SDK happy path.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct State {
    pub(crate) cells: Vec<LogicalStateCell>,
}

impl State {
    pub(crate) fn from_cells(cells: Vec<LogicalStateCell>) -> Self {
        Self { cells }
    }

    pub(crate) fn cells_raw(&self) -> &[LogicalStateCell] {
        &self.cells
    }

    pub(crate) fn upsert(&mut self, cell: LogicalStateCell) {
        if let Some(existing) = self.cells.iter_mut().find(|existing| {
            existing.table == cell.table && existing.field == cell.field && existing.key == cell.key
        }) {
            *existing = cell;
        } else {
            self.cells.push(cell);
        }
    }

    /// Iterate over all logical state cells in the snapshot.
    pub fn cells(&self) -> impl Iterator<Item = &LogicalStateCell> {
        self.cells.iter()
    }

    /// Remove a cell from the snapshot by `(table, key tuple, field)`.
    pub fn remove(
        &mut self,
        table: tabula_ir::TableId,
        key: &[PortableValue],
        field: tabula_ir::FieldId,
    ) {
        self.cells
            .retain(|cell| !(cell.table == table && cell.field == field && cell.key == key));
    }
}
