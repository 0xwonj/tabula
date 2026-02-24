#![warn(missing_docs)]
#![deny(unused)]

//! Canonical artifact models and helpers shared by adapters and orchestration.

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::{
    CellKey, ColId, ExecutionConsistencyStatus, RowKey, TableId, TableSchema, Transaction,
    TxTypeId, Value,
};
use tabula_ir::TxTypeDef;

/// Artifact-layer error.
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// Invalid hex sender format.
    #[error("invalid sender hex ({context}): {detail}")]
    InvalidSenderHex {
        /// What went wrong (e.g., "length", "encoding", "hex digit").
        context: &'static str,
        /// Human-readable detail.
        detail: String,
    },
    /// Missing state value for a required cell.
    #[error("state cell is missing value (table={table}, row={row}, col={col})")]
    MissingStateValue {
        /// Table id.
        table: u32,
        /// Row key.
        row: u64,
        /// Column id.
        col: u16,
    },
    /// JSON file read error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to read {path}: {source}")]
    ReadJson {
        /// File path.
        path: String,
        /// Source error.
        source: std::io::Error,
    },
    /// JSON file parse error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to parse {path}: {source}")]
    ParseJson {
        /// File path.
        path: String,
        /// Source error.
        source: serde_json::Error,
    },
    /// JSON file write error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to write {path}: {source}")]
    WriteJson {
        /// File path.
        path: String,
        /// Source error.
        source: std::io::Error,
    },
    /// JSON serialization error.
    #[error("failed to encode JSON: {0}")]
    EncodeJson(#[from] serde_json::Error),
}

/// Program artifact used by compile/check/execute interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramArtifact {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Optional metadata envelope (required for JSON artifact mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_metadata: Option<ContractMetadataEnvelope>,
}

/// Execution receipt model (non-STARK placeholder backend).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    /// Receipt version.
    pub version: u32,
    /// Scheme identifier.
    pub scheme: String,
    /// Statement hash.
    pub statement_hash: String,
    /// Program hash.
    pub program_hash: String,
    /// State hash.
    pub state_hash: String,
    /// Batch hash.
    pub batch_hash: String,
    /// Output state hash.
    pub state_after_hash: String,
    /// Metadata hash.
    pub metadata_hash: String,
    /// Generation timestamp.
    pub generated_at_ms: u64,
    /// Number of transactions.
    pub tx_count: usize,
    /// Number of emitted events.
    pub emitted_count: usize,
    /// Consistency status.
    pub consistency: ExecutionConsistencyStatus,
}

/// Summary of one chip in a STARK proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipSummary {
    /// Chip name.
    pub name: String,
    /// Trace height (number of rows).
    pub trace_height: usize,
}

/// Serializable summary of a STARK proof (the actual proof is not serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarkProofSummary {
    /// Scheme identifier.
    pub scheme: String,
    /// Whether the proof was verified.
    pub verified: bool,
    /// Number of chips.
    pub chip_count: usize,
    /// Per-chip summaries.
    pub chips: Vec<ChipSummary>,
    /// Old state root (8 BabyBear field elements as hex strings).
    pub old_state_root: Vec<String>,
    /// New state root (8 BabyBear field elements as hex strings).
    pub new_state_root: Vec<String>,
    /// Proving time in milliseconds.
    pub prove_time_ms: u64,
    /// Verification time in milliseconds.
    pub verify_time_ms: u64,
    /// Statement hash (hex).
    pub statement_hash: String,
    /// Program hash (hex).
    pub program_hash: String,
    /// Batch hash (hex).
    pub batch_hash: String,
}

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

/// JSON representation of a transaction batch file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchFile {
    /// Transactions in execution order.
    pub transactions: Vec<TxInput>,
}

/// One transaction input row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TxInput {
    /// Transaction type id.
    pub tx_type: u32,
    /// Typed transaction params.
    pub params: Vec<Value>,
    /// Sender as hex-encoded 32-byte key.
    pub sender: String,
    /// Replay nonce.
    pub nonce: u64,
}

impl TxInput {
    /// Convert to core transaction form.
    pub fn to_transaction(&self) -> Result<Transaction, ArtifactError> {
        let sender = parse_hex_32(&self.sender)?;
        Ok(Transaction {
            tx_type: TxTypeId(self.tx_type),
            params: self.params.clone(),
            sender,
            nonce: self.nonce,
            signature: vec![],
        })
    }
}

/// Parse a hex-encoded 32-byte value, left-padding short strings.
pub fn parse_hex_32(s: &str) -> Result<[u8; 32], ArtifactError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Ok([0u8; 32]);
    }

    let padded = format!("{:0>64}", s);
    if padded.len() != 64 {
        return Err(ArtifactError::InvalidSenderHex {
            context: "length",
            detail: format!("expected at most 64 hex chars, got {}", s.len()),
        });
    }

    let mut out = [0u8; 32];
    for (i, chunk) in padded.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| ArtifactError::InvalidSenderHex {
            context: "encoding",
            detail: e.to_string(),
        })?;
        out[i] = u8::from_str_radix(byte_str, 16).map_err(|e| ArtifactError::InvalidSenderHex {
            context: "hex digit",
            detail: e.to_string(),
        })?;
    }
    Ok(out)
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

/// Read and parse JSON from file.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ArtifactError> {
    let path_str = path.display().to_string();
    let content = std::fs::read_to_string(path).map_err(|source| ArtifactError::ReadJson {
        path: path_str.clone(),
        source,
    })?;
    serde_json::from_str(&content).map_err(|source| ArtifactError::ParseJson {
        path: path_str,
        source,
    })
}

/// Serialize and write pretty JSON to file.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), ArtifactError> {
    let path_str = path.display().to_string();
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, content).map_err(|source| ArtifactError::WriteJson {
        path: path_str,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_32_full() {
        let hex = "01".repeat(32);
        let out = parse_hex_32(&hex).expect("hex should parse");
        assert_eq!(out, [1u8; 32]);
    }

    #[test]
    fn parse_hex_32_short_left_pad() {
        let out = parse_hex_32("ff").expect("hex should parse");
        let mut expected = [0u8; 32];
        expected[31] = 0xff;
        assert_eq!(out, expected);
    }

    #[test]
    fn parse_hex_32_with_prefix() {
        let out = parse_hex_32("0xff").expect("hex with prefix");
        let mut expected = [0u8; 32];
        expected[31] = 0xff;
        assert_eq!(out, expected);
    }

    #[test]
    fn parse_hex_32_empty() {
        let out = parse_hex_32("").expect("empty hex");
        assert_eq!(out, [0u8; 32]);
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
    fn batch_file_serde_roundtrip() {
        let batch = BatchFile {
            transactions: vec![TxInput {
                tx_type: 0,
                params: vec![Value::U64(100)],
                sender: "01".repeat(32),
                nonce: 0,
            }],
        };

        let json = serde_json::to_string(&batch).expect("serialize");
        let back: BatchFile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.transactions.len(), 1);
        assert_eq!(back.transactions[0].params[0], Value::U64(100));
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
        // BTreeMap produces sorted output by (table, row, col).
        assert_eq!(normalized.cells[0].row, 0);
        assert_eq!(normalized.cells[0].value, Some(Value::U64(20)));
        assert_eq!(normalized.cells[1].row, 1);
        // Last write wins for row=1.
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
    fn tx_input_to_transaction_roundtrip() {
        let tx = TxInput {
            tx_type: 1,
            params: vec![Value::U64(42), Value::Bool(true)],
            sender: "ab".repeat(32),
            nonce: 7,
        };

        let core_tx = tx.to_transaction().expect("convert");
        assert_eq!(core_tx.tx_type, TxTypeId(1));
        assert_eq!(core_tx.params.len(), 2);
        assert_eq!(core_tx.nonce, 7);
        assert_eq!(core_tx.sender[0], 0xab);
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
