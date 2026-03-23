//! State models and utilities.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use tabula_core::{CellKey, ColId, PortableValue, RowKey, TableId};
use tabula_types::TypeRuntimeRegistry;

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

/// Canonical state value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
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
    pub value: Option<PortableValue>,
}

impl State {
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
    /// Convert to a portable `(CellKey, PortableValue)` pair.
    pub fn to_cell_pair(
        &self,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<(CellKey, PortableValue), ArtifactError> {
        let key = CellKey {
            table: TableId(self.table),
            col: ColId(self.col),
            row: RowKey(self.row),
        };
        let Some(value) = &self.value else {
            return Err(ArtifactError::MissingStateValue {
                table: self.table,
                row: self.row,
                col: self.col,
            });
        };
        type_runtimes.decode_portable(value).map_err(|err| {
            ArtifactError::InvalidPortableValue {
                detail: err.to_string(),
            }
        })?;
        Ok((key, value.clone()))
    }

    /// Build a JSON state cell from portable key/value.
    pub fn from_cell_pair(key: &CellKey, value: &Option<PortableValue>) -> Self {
        Self {
            table: key.table.0,
            row: key.row.0,
            col: key.col.0,
            value: value.clone(),
        }
    }
}

/// Merge a write-set over initial state cells with last-write-wins semantics.
pub fn merge_output_state_entries(
    initial_cells: &[StateEntry],
    write_set_final: &[(CellKey, Option<PortableValue>)],
) -> Vec<StateEntry> {
    let mut merged: BTreeMap<(u32, u64, u16), PortableValue> = BTreeMap::new();

    for cell in initial_cells {
        if let Some(value) = &cell.value {
            merged.insert((cell.table, cell.row, cell.col), value.clone());
        }
    }

    for (key, value) in write_set_final {
        let tuple_key = (key.table.0, key.row.0, key.col.0);
        match value {
            Some(v) => {
                merged.insert(tuple_key, v.clone());
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

/// Normalize a state by deduplicating cells on `(table, row, col)`.
///
/// When multiple cells share the same key, the last one wins. Each resulting
/// cell has a non-`None` value.
pub fn normalize_state(input: &State) -> Result<State, ArtifactError> {
    let mut merged = BTreeMap::new();
    for cell in &input.cells {
        let Some(value) = &cell.value else {
            return Err(ArtifactError::MissingStateValue {
                table: cell.table,
                row: cell.row,
                col: cell.col,
            });
        };
        merged.insert((cell.table, cell.row, cell.col), value.clone());
    }

    Ok(State {
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
    use tabula_types::{TypeRuntimeRegistry, bool_portable, i64_portable, u64_portable};

    #[test]
    fn state_file_serde_roundtrip() {
        let state = State {
            cells: vec![
                StateEntry {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(u64_portable(42)),
                },
                StateEntry {
                    table: 1,
                    row: 5,
                    col: 2,
                    value: Some(bool_portable(true)),
                },
            ],
        };

        let json = serde_json::to_string(&state).expect("serialize");
        let back: State = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.cells.len(), 2);
        assert_eq!(back.cells[0].value, Some(u64_portable(42)));
        assert_eq!(back.cells[1].value, Some(bool_portable(true)));
    }

    #[test]
    fn merge_output_state_entries_deduplicates_initial_entries() {
        let initial = vec![
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(u64_portable(10)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(u64_portable(20)),
            },
        ];

        let merged = merge_output_state_entries(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(u64_portable(20)));
    }

    #[test]
    fn merge_output_state_entries_applies_write_set() {
        let initial = vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(u64_portable(100)),
        }];
        let write_set = vec![(
            CellKey {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(0),
            },
            Some(u64_portable(200)),
        )];

        let merged = merge_output_state_entries(&initial, &write_set);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(u64_portable(200)));
    }

    #[test]
    fn merge_output_state_entries_delete_removes_entry() {
        let initial = vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(u64_portable(100)),
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
        let state = State {
            cells: vec![
                StateEntry {
                    table: 0,
                    row: 1,
                    col: 0,
                    value: Some(u64_portable(10)),
                },
                StateEntry {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(u64_portable(20)),
                },
                StateEntry {
                    table: 0,
                    row: 1,
                    col: 0,
                    value: Some(u64_portable(30)),
                },
            ],
        };

        let normalized = normalize_state(&state).expect("normalize");
        assert_eq!(normalized.cells.len(), 2);
        assert_eq!(normalized.cells[0].row, 0);
        assert_eq!(normalized.cells[0].value, Some(u64_portable(20)));
        assert_eq!(normalized.cells[1].row, 1);
        assert_eq!(normalized.cells[1].value, Some(u64_portable(30)));
    }

    #[test]
    fn normalize_state_rejects_null_values() {
        let state = State {
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
        let value = Some(i64_portable(-42));
        let cell = StateEntry::from_cell_pair(&key, &value);
        assert_eq!(cell.table, 1);
        assert_eq!(cell.row, 3);
        assert_eq!(cell.col, 2);
        assert_eq!(cell.value, Some(i64_portable(-42)));

        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let (back_key, back_val) = cell.to_cell_pair(&runtimes).expect("back");
        assert_eq!(back_key, key);
        assert_eq!(back_val, i64_portable(-42));
    }

    #[test]
    fn canonical_digest_normalizes_equivalent_states() {
        let left = State {
            cells: vec![
                StateEntry {
                    table: 1,
                    row: 0,
                    col: 0,
                    value: Some(u64_portable(1)),
                },
                StateEntry {
                    table: 1,
                    row: 0,
                    col: 0,
                    value: Some(u64_portable(2)),
                },
            ],
        };
        let right = State {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: Some(u64_portable(2)),
            }],
        };

        assert_eq!(
            left.canonical_digest().expect("left digest"),
            right.canonical_digest().expect("right digest")
        );
    }
}
