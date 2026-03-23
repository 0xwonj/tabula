//! Canonical state fixtures for black-box tests.

use tabula_artifact::{State, StateEntry};
use tabula_core::{CellKey, ColId, RowKey, TableId};
use tabula_types::u64_portable;

pub fn cell_key(table: u32, row: u64, col: u16) -> CellKey {
    CellKey {
        table: TableId(table),
        col: ColId(col),
        row: RowKey(row),
    }
}

pub fn empty_state() -> State {
    State { cells: vec![] }
}

pub fn single_cell_u64(table: TableId, col: ColId, row: RowKey, value: u64) -> State {
    State {
        cells: vec![StateEntry {
            table: table.0,
            row: row.0,
            col: col.0,
            value: Some(u64_portable(value)),
        }],
    }
}

pub fn two_account_balances(a: u64, b: u64) -> State {
    State {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(u64_portable(a)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 0,
                value: Some(u64_portable(b)),
            },
        ],
    }
}

pub fn three_account_balances(a: u64, b: u64, c: u64) -> State {
    State {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(u64_portable(a)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 0,
                value: Some(u64_portable(b)),
            },
            StateEntry {
                table: 0,
                row: 2,
                col: 0,
                value: Some(u64_portable(c)),
            },
        ],
    }
}

pub fn liquid_shielded_state(liquid: u64, shielded: u64) -> State {
    State {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(u64_portable(liquid)),
            },
            StateEntry {
                table: 0,
                row: 0,
                col: 1,
                value: Some(u64_portable(shielded)),
            },
        ],
    }
}
