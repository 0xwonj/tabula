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
    /// Returns an error if value is `None`.
    pub fn to_cell_pair(&self) -> anyhow::Result<(CellKey, Value)> {
        let key = CellKey {
            table: TableId(self.table),
            col: ColId(self.col),
            row: RowKey(self.row),
        };
        let Some(value) = self.value else {
            anyhow::bail!(
                "state cell is missing value (table={}, row={}, col={})",
                self.table,
                self.row,
                self.col
            );
        };
        Ok((key, value))
    }

    /// Create from a `(CellKey, Option<Value>)` pair.
    pub fn from_cell_pair(key: &CellKey, value: &Option<Value>) -> Self {
        Self {
            table: key.table.0,
            row: key.row.0,
            col: key.col.0,
            value: *value,
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
// Program loading helpers
// ---------------------------------------------------------------------------

/// Load program sources from a `.tab` or `.json` file.
///
/// Returns (schemas, tx_types) regardless of input format.
pub(crate) fn load_program_sources(
    path: &std::path::Path,
) -> anyhow::Result<(Vec<TableSchema>, Vec<TxTypeDef>)> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "tab" {
        let source = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        match tabula_lang::compile(&source) {
            Ok(compiled) => Ok((compiled.schemas, compiled.tx_types)),
            Err(errors) => {
                let mut msg = String::new();
                for err in &errors {
                    if !msg.is_empty() {
                        msg.push_str("\n\n");
                    }
                    msg.push_str(&format!("{}", err.display_with_source(&source)));
                }
                Err(anyhow::anyhow!("{msg}"))
            }
        }
    } else {
        let pf: ProgramFile = load_json(path)?;
        Ok((pf.table_schemas, pf.tx_types))
    }
}

/// Register schemas and tx types into a `Program` (with NF validation).
pub(crate) fn register_program(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
) -> anyhow::Result<tabula_ir::Program> {
    validate_schema_coverage(schemas, tx_types)?;

    let mut program = tabula_ir::Program::new();
    for schema in schemas {
        program.add_schema(schema.clone());
    }
    for def in tx_types {
        program.register(def.clone())?;
    }
    Ok(program)
}

/// Validate that every state/static-table access has a declared schema+column.
fn validate_schema_coverage(schemas: &[TableSchema], tx_types: &[TxTypeDef]) -> anyhow::Result<()> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut columns_by_table: BTreeMap<TableId, BTreeSet<ColId>> = BTreeMap::new();
    for schema in schemas {
        let cols = columns_by_table.entry(schema.id).or_default();
        for col in &schema.columns {
            cols.insert(col.id);
        }
    }

    for tx in tx_types {
        for (instr_idx, instr) in tx.body.iter().enumerate() {
            match instr {
                tabula_ir::Instruction::Read { table, col, .. }
                | tabula_ir::Instruction::Write { table, col, .. } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *table, *col)?
                }
                tabula_ir::Instruction::Lookup {
                    static_table, col, ..
                } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *static_table, *col)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn ensure_table_col_exists(
    columns_by_table: &std::collections::BTreeMap<TableId, std::collections::BTreeSet<ColId>>,
    tx: &TxTypeDef,
    instr_idx: usize,
    table: TableId,
    col: ColId,
) -> anyhow::Result<()> {
    let Some(cols) = columns_by_table.get(&table) else {
        anyhow::bail!(
            "tx '{}' (id {}), instruction {} references table {} but no schema is declared for it",
            tx.name,
            tx.id.0,
            instr_idx,
            table.0
        );
    };
    if !cols.contains(&col) {
        anyhow::bail!(
            "tx '{}' (id {}), instruction {} references table {} col {} but that column is missing in schema",
            tx.name,
            tx.id.0,
            instr_idx,
            table.0,
            col.0
        );
    }
    Ok(())
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
    use tabula_core::{ColumnDef, ValueType};
    use tabula_ir::{Instruction, RowExpr, TxTypeDef};

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

    #[test]
    fn test_state_cell_to_cell_pair_rejects_null() {
        let cell = StateCell {
            table: 1,
            row: 2,
            col: 3,
            value: None,
        };
        assert!(cell.to_cell_pair().is_err());
    }

    #[test]
    fn test_register_program_rejects_missing_schema_for_accessed_table() {
        let tx = TxTypeDef {
            id: TxTypeId(0),
            name: "read".into(),
            param_schema: vec![],
            body: vec![Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(10),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            }],
        };
        let err = register_program(&[], &[tx]).unwrap_err();
        assert!(err.to_string().contains("no schema"));
    }

    #[test]
    fn test_register_program_rejects_missing_column_in_schema() {
        let schemas = vec![TableSchema {
            id: TableId(10),
            name: "t".into(),
            columns: vec![ColumnDef {
                id: ColId(1),
                name: "x".into(),
                value_type: ValueType::U64,
            }],
        }];
        let tx = TxTypeDef {
            id: TxTypeId(0),
            name: "read".into(),
            param_schema: vec![],
            body: vec![Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(10),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            }],
        };
        let err = register_program(&schemas, &[tx]).unwrap_err();
        assert!(err.to_string().contains("column is missing"));
    }
}
