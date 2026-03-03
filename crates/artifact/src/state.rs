//! State file models and utilities.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use tabula_core::{CellKey, ColId, RowKey, TableId, Value};

use crate::ArtifactError;

/// JSON representation of a state file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    /// All state cells.
    pub cells: Vec<StateCell>,
}

/// One logical state cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateCell {
    /// Table id.
    pub table: u32,
    /// Row key.
    pub row: u64,
    /// Column id.
    pub col: u16,
    /// Optional value (`null` means delete/absence).
    pub value: Option<Value>,
}

impl StateCell {
    /// Convert to a typed `(CellKey, Value)` pair.
    pub fn to_cell_pair(&self) -> Result<(CellKey, Value), ArtifactError> {
        let key = CellKey {
            table: TableId(self.table),
            col: ColId(self.col),
            row: RowKey(self.row),
        };
        let Some(value) = self.value else {
            return Err(ArtifactError::MissingStateValue {
                table: self.table,
                row: self.row,
                col: self.col,
            });
        };
        Ok((key, value))
    }

    /// Build a JSON state cell from typed key/value.
    pub fn from_cell_pair(key: &CellKey, value: &Option<Value>) -> Self {
        Self {
            table: key.table.0,
            row: key.row.0,
            col: key.col.0,
            value: *value,
        }
    }
}

/// Merge a write-set over initial state cells with last-write-wins semantics.
pub fn merge_output_state_cells(
    initial_cells: &[StateCell],
    write_set_final: &[(CellKey, Option<Value>)],
) -> Vec<StateCell> {
    let mut merged: BTreeMap<(u32, u64, u16), Value> = BTreeMap::new();

    for cell in initial_cells {
        if let Some(value) = cell.value {
            merged.insert((cell.table, cell.row, cell.col), value);
        }
    }

    for (key, value) in write_set_final {
        let tuple_key = (key.table.0, key.row.0, key.col.0);
        match value {
            Some(v) => {
                merged.insert(tuple_key, *v);
            }
            None => {
                merged.remove(&tuple_key);
            }
        }
    }

    merged
        .into_iter()
        .map(|((table, row, col), value)| StateCell {
            table,
            row,
            col,
            value: Some(value),
        })
        .collect()
}

/// Normalize a state file by deduplicating cells on `(table, row, col)`.
///
/// When multiple cells share the same key, the last one wins. Each resulting
/// cell has a non-`None` value.
pub fn normalize_state(input: &StateFile) -> Result<StateFile, ArtifactError> {
    let mut merged = BTreeMap::new();
    for cell in &input.cells {
        let (key, value) = cell.to_cell_pair()?;
        merged.insert((key.table.0, key.row.0, key.col.0), value);
    }

    Ok(StateFile {
        cells: merged
            .into_iter()
            .map(|((table, row, col), value)| StateCell {
                table,
                row,
                col,
                value: Some(value),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_file_serde_roundtrip() {
        let state = StateFile {
            cells: vec![
                StateCell {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(42)),
                },
                StateCell {
                    table: 1,
                    row: 5,
                    col: 2,
                    value: Some(Value::Bool(true)),
                },
            ],
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let back: StateFile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.cells.len(), 2);
        assert_eq!(back.cells[0].value, Some(Value::U64(42)));
        assert_eq!(back.cells[1].value, Some(Value::Bool(true)));
    }

    #[test]
    fn merge_output_state_cells_deduplicates_initial_cells() {
        let initial = vec![
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(10)),
            },
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(20)),
            },
        ];

        let merged = merge_output_state_cells(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(20)));
    }

    #[test]
    fn merge_output_state_cells_applies_write_set() {
        let initial = vec![StateCell {
            table: 0,
            row: 0,
            col: 0,
            value: Some(Value::U64(100)),
        }];
        let write_set = vec![(
            CellKey {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(0),
            },
            Some(Value::U64(200)),
        )];

        let merged = merge_output_state_cells(&initial, &write_set);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(200)));
    }

    #[test]
    fn merge_output_state_cells_delete_removes_cell() {
        let initial = vec![StateCell {
            table: 0,
            row: 0,
            col: 0,
            value: Some(Value::U64(100)),
        }];
        let write_set = vec![(
            CellKey {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(0),
            },
            None,
        )];

        let merged = merge_output_state_cells(&initial, &write_set);
        assert!(merged.is_empty());
    }

    #[test]
    fn normalize_state_deduplicates_and_sorts() {
        let state = StateFile {
            cells: vec![
                StateCell {
                    table: 0,
                    row: 1,
                    col: 0,
                    value: Some(Value::U64(10)),
                },
                StateCell {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(20)),
                },
                StateCell {
                    table: 0,
                    row: 1,
                    col: 0,
                    value: Some(Value::U64(30)),
                },
            ],
        };

        let normalized = normalize_state(&state).expect("normalize");
        assert_eq!(normalized.cells.len(), 2);
        assert_eq!(normalized.cells[0].row, 0);
        assert_eq!(normalized.cells[0].value, Some(Value::U64(20)));
        assert_eq!(normalized.cells[1].row, 1);
        assert_eq!(normalized.cells[1].value, Some(Value::U64(30)));
    }

    #[test]
    fn normalize_state_rejects_null_values() {
        let state = StateFile {
            cells: vec![StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: None,
            }],
        };

        let result = normalize_state(&state);
        assert!(result.is_err());
    }

    #[test]
    fn state_cell_from_cell_pair_roundtrip() {
        let key = CellKey {
            table: TableId(1),
            col: ColId(2),
            row: RowKey(3),
        };
        let value = Some(Value::I64(-42));
        let cell = StateCell::from_cell_pair(&key, &value);
        assert_eq!(cell.table, 1);
        assert_eq!(cell.row, 3);
        assert_eq!(cell.col, 2);
        assert_eq!(cell.value, Some(Value::I64(-42)));

        let (back_key, back_val) = cell.to_cell_pair().expect("back");
        assert_eq!(back_key, key);
        assert_eq!(back_val, Value::I64(-42));
    }
}
