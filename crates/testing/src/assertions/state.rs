use std::collections::BTreeMap;

use tabula_artifact::{StateSnapshot, normalize_state};
use tabula_core::{CellKey, ColId, RowKey, TableId, Value};

/// Canonical state-cell expectation used by semantic assertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedStateCell {
    pub table: TableId,
    pub col: ColId,
    pub row: RowKey,
    pub value: Option<Value>,
}

fn normalized_state_map(state: &StateSnapshot) -> BTreeMap<CellKey, Option<Value>> {
    let normalized = normalize_state(state).expect("normalize state for assertion");
    normalized
        .cells
        .into_iter()
        .map(|entry| {
            (
                CellKey {
                    table: TableId(entry.table),
                    col: ColId(entry.col),
                    row: RowKey(entry.row),
                },
                entry.value,
            )
        })
        .collect()
}

/// Assert that one logical cell matches the expected value after normalization.
pub fn assert_state_cell(
    state: &StateSnapshot,
    table: TableId,
    col: ColId,
    row: RowKey,
    expected: Option<Value>,
) {
    let actual = normalized_state_map(state)
        .get(&CellKey { table, col, row })
        .copied()
        .flatten();
    assert_eq!(
        actual, expected,
        "state cell mismatch at ({}, {}, {})",
        table.0, col.0, row.0
    );
}

/// Assert that a state snapshot contains exactly the expected cells after normalization.
pub fn assert_state_cells_exact(state: &StateSnapshot, expected_cells: &[ExpectedStateCell]) {
    let actual = normalized_state_map(state);
    let expected: BTreeMap<_, _> = expected_cells
        .iter()
        .map(|cell| {
            (
                CellKey {
                    table: cell.table,
                    col: cell.col,
                    row: cell.row,
                },
                cell.value,
            )
        })
        .collect();
    assert_eq!(actual, expected, "normalized state cells differ");
}
