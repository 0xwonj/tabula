//! State snapshot models and utilities.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use tabula_core::{CellKey, ColId, RowKey, TableId, Value};

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

/// Canonical state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSnapshot {
    /// All state cells.
    pub cells: Vec<StateEntry>,
}

/// One logical state cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateEntry {
    /// Table id.
    pub table: u32,
    /// Row key.
    pub row: u64,
    /// Column id.
    pub col: u16,
    /// Optional value (`null` means delete/absence).
    pub value: Option<Value>,
}

impl StateSnapshot {
    /// Serialize this state into canonical bytes after normalization.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        let normalized = normalize_state(self)?;
        canonical_json_bytes(&normalized)
    }

    /// Compute the canonical digest bytes after normalization.
    pub fn canonical_digest_bytes(&self) -> Result<[u8; 32], ArtifactError> {
        let normalized = normalize_state(self)?;
        canonical_json_digest("state", &normalized)
    }

    /// Compute the canonical digest hex string after normalization.
    pub fn canonical_digest(&self) -> Result<String, ArtifactError> {
        Ok(bytes_to_hex(&self.canonical_digest_bytes()?))
    }
}

impl StateEntry {
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
pub fn merge_output_state_entries(
    initial_cells: &[StateEntry],
    write_set_final: &[(CellKey, Option<Value>)],
) -> Vec<StateEntry> {
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
        .map(|((table, row, col), value)| StateEntry {
            table,
            row,
            col,
            value: Some(value),
        })
        .collect()
}

/// Normalize a state snapshot by deduplicating cells on `(table, row, col)`.
///
/// When multiple cells share the same key, the last one wins. Each resulting
/// cell has a non-`None` value.
pub fn normalize_state(input: &StateSnapshot) -> Result<StateSnapshot, ArtifactError> {
    let mut merged = BTreeMap::new();
    for cell in &input.cells {
        let (key, value) = cell.to_cell_pair()?;
        merged.insert((key.table.0, key.row.0, key.col.0), value);
    }

    Ok(StateSnapshot {
        cells: merged
            .into_iter()
            .map(|((table, row, col), value)| StateEntry {
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
        let state = StateSnapshot {
            cells: vec![
                StateEntry {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(42)),
                },
                StateEntry {
                    table: 1,
                    row: 5,
                    col: 2,
                    value: Some(Value::Bool(true)),
                },
            ],
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let back: StateSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.cells.len(), 2);
        assert_eq!(back.cells[0].value, Some(Value::U64(42)));
        assert_eq!(back.cells[1].value, Some(Value::Bool(true)));
    }

    #[test]
    fn merge_output_state_entries_deduplicates_initial_entries() {
        let initial = vec![
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(10)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(20)),
            },
        ];

        let merged = merge_output_state_entries(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(20)));
    }

    #[test]
    fn merge_output_state_entries_applies_write_set() {
        let initial = vec![StateEntry {
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

        let merged = merge_output_state_entries(&initial, &write_set);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(200)));
    }

    #[test]
    fn merge_output_state_entries_delete_removes_entry() {
        let initial = vec![StateEntry {
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

        let merged = merge_output_state_entries(&initial, &write_set);
        assert!(merged.is_empty());
    }

    #[test]
    fn normalize_state_deduplicates_and_sorts() {
        let state = StateSnapshot {
            cells: vec![
                StateEntry {
                    table: 0,
                    row: 1,
                    col: 0,
                    value: Some(Value::U64(10)),
                },
                StateEntry {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(20)),
                },
                StateEntry {
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
        let state = StateSnapshot {
            cells: vec![StateEntry {
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
        let cell = StateEntry::from_cell_pair(&key, &value);
        assert_eq!(cell.table, 1);
        assert_eq!(cell.row, 3);
        assert_eq!(cell.col, 2);
        assert_eq!(cell.value, Some(Value::I64(-42)));

        let (back_key, back_val) = cell.to_cell_pair().expect("back");
        assert_eq!(back_key, key);
        assert_eq!(back_val, Value::I64(-42));
    }

    #[test]
    fn canonical_digest_normalizes_equivalent_states() {
        let left = StateSnapshot {
            cells: vec![
                StateEntry {
                    table: 1,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(1)),
                },
                StateEntry {
                    table: 1,
                    row: 0,
                    col: 0,
                    value: Some(Value::U64(2)),
                },
            ],
        };
        let right = StateSnapshot {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: Some(Value::U64(2)),
            }],
        };

        assert_eq!(
            left.canonical_digest().expect("left digest"),
            right.canonical_digest().expect("right digest")
        );
    }
}
