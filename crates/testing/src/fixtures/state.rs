//! Canonical state fixtures for black-box tests.

use tabula_artifact::{StateEntry, StateSnapshot};
use tabula_core::{CellKey, ColId, RowKey, TableId, Value};

pub fn cell_key(table: u32, row: u64, col: u16) -> CellKey {
    CellKey {
        table: TableId(table),
        col: ColId(col),
        row: RowKey(row),
    }
}

pub fn empty_state() -> StateSnapshot {
    StateSnapshot { cells: vec![] }
}

pub fn single_cell_u64(table: TableId, col: ColId, row: RowKey, value: u64) -> StateSnapshot {
    StateSnapshot {
        cells: vec![StateEntry {
            table: table.0,
            row: row.0,
            col: col.0,
            value: Some(Value::U64(value)),
        }],
    }
}

pub fn two_account_balances(a: u64, b: u64) -> StateSnapshot {
    StateSnapshot {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(a)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 0,
                value: Some(Value::U64(b)),
            },
        ],
    }
}

pub fn three_account_balances(a: u64, b: u64, c: u64) -> StateSnapshot {
    StateSnapshot {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(a)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 0,
                value: Some(Value::U64(b)),
            },
            StateEntry {
                table: 0,
                row: 2,
                col: 0,
                value: Some(Value::U64(c)),
            },
        ],
    }
}

pub fn liquid_shielded_state(liquid: u64, shielded: u64) -> StateSnapshot {
    StateSnapshot {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(liquid)),
            },
            StateEntry {
                table: 0,
                row: 0,
                col: 1,
                value: Some(Value::U64(shielded)),
            },
        ],
    }
}
