//! JSON input/output types for the CLI.
//!
//! Wraps core types with user-friendly JSON serialization
//! (hex strings for byte arrays, flat cell format for state).

use serde::{Deserialize, Serialize};

use tabula_core::{
    CellKey, ColId, EmittedEvent, ExecutionEvent, RowKey, TableId, TableSchema, Transaction,
    TxOutcome, TxTypeId, Value,
};
use tabula_ir::TxTypeDef;

// ---------------------------------------------------------------------------
// Program input
// ---------------------------------------------------------------------------

/// JSON representation of a program file.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProgramFile {
    /// Table schema definitions for type inference (omitted = empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
}

// ---------------------------------------------------------------------------
// State input/output
// ---------------------------------------------------------------------------

/// JSON representation of a state file.
#[derive(Debug, Serialize, Deserialize)]
pub struct StateFile {
    /// All cell values in the state.
    pub cells: Vec<StateCell>,
}

/// A single cell entry in the state file.
///
/// `value` is `None` when the cell is absent (deleted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCell {
    /// Table ID.
    pub table: u32,
    /// Row key.
    pub row: u64,
    /// Column ID.
    pub col: u16,
    /// Cell value (`null` in JSON = absent).
    pub value: Option<Value>,
}

impl StateCell {
    /// Convert to a `(CellKey, Value)` pair.
    ///
    /// Panics if value is `None` — only use for state file input where cells
    /// are always present.
    pub fn to_cell_pair(&self) -> (CellKey, Value) {
        (
            CellKey {
                table: TableId(self.table),
                col: ColId(self.col),
                row: RowKey(self.row),
            },
            self.value
                .clone()
                .expect("state cell value must be present"),
        )
    }

    /// Create from a `(CellKey, Option<Value>)` pair.
    pub fn from_cell_pair(key: &CellKey, value: &Option<Value>) -> Self {
        Self {
            table: key.table.0,
            row: key.row.0,
            col: key.col.0,
            value: value.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Batch input
// ---------------------------------------------------------------------------

/// JSON representation of a batch file.
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchFile {
    /// Transactions to execute.
    pub transactions: Vec<TxInput>,
}

/// A single transaction in the batch file.
///
/// Uses hex strings for sender (instead of `[u8; 32]` arrays).
/// Signature is omitted (empty) since Phase 1 uses `MockSigVerifier`.
#[derive(Debug, Serialize, Deserialize)]
pub struct TxInput {
    /// Transaction type ID.
    pub tx_type: u32,
    /// Parameter values.
    pub params: Vec<Value>,
    /// Sender public key as hex string (64 hex chars = 32 bytes).
    pub sender: String,
    /// Replay-protection nonce.
    pub nonce: u64,
}

impl TxInput {
    /// Convert to a core `Transaction`.
    pub fn to_transaction(&self) -> anyhow::Result<Transaction> {
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

// ---------------------------------------------------------------------------
// Execution output
// ---------------------------------------------------------------------------

/// JSON representation of execution results.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionOutput {
    /// Per-transaction outcomes.
    pub tx_outcomes: Vec<TxOutcome>,
    /// Cells read from committed state.
    pub read_set: Vec<StateCell>,
    /// Final writes to committed state.
    pub write_set: Vec<StateCell>,
    /// Emitted application events.
    pub emitted: Vec<EmittedEvent>,
    /// Consistency check result.
    pub consistency: String,
    /// Full execution trace (only if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<ExecutionEvent>>,
}

// ---------------------------------------------------------------------------
// JSON I/O helpers
// ---------------------------------------------------------------------------

/// Deserialize a JSON file from the given path.
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> anyhow::Result<T> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", path.display()))
}

/// Serialize a value to a pretty-printed JSON file.
pub(crate) fn write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, content)
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Hex helpers
// ---------------------------------------------------------------------------

/// Parse a hex string into a `[u8; 32]` array.
///
/// Accepts with or without `0x` prefix. If the string is shorter than
/// 64 hex chars, it is zero-padded on the left.
fn parse_hex_32(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Ok([0u8; 32]);
    }
    // Pad to 64 hex chars
    let padded = format!("{:0>64}", s);
    if padded.len() != 64 {
        anyhow::bail!(
            "hex string too long: expected at most 64 hex chars, got {}",
            s.len()
        );
    }
    let mut out = [0u8; 32];
    for (i, chunk) in padded.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk)?;
        out[i] = u8::from_str_radix(byte_str, 16)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_32_full() {
        let hex = "01".repeat(32);
        let result = parse_hex_32(&hex).unwrap();
        assert_eq!(result, [1u8; 32]);
    }

    #[test]
    fn test_parse_hex_32_with_prefix() {
        let hex = format!("0x{}", "ab".repeat(32));
        let result = parse_hex_32(&hex).unwrap();
        assert_eq!(result, [0xab; 32]);
    }

    #[test]
    fn test_parse_hex_32_short_padded() {
        let result = parse_hex_32("ff").unwrap();
        let mut expected = [0u8; 32];
        expected[31] = 0xff;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_hex_32_empty() {
        let result = parse_hex_32("").unwrap();
        assert_eq!(result, [0u8; 32]);
    }
}
