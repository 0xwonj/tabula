use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::{
    CellKey, ColId, EmittedEvent, ExecutionConsistencyStatus, ExecutionEvent, RowKey, TableId,
    TableSchema, Transaction, TxOutcome, TxTypeId, Value,
};
use tabula_ir::TxTypeDef;

#[derive(Debug, Clone)]
pub enum InputRef<T> {
    Inline(T),
    File(PathBuf),
    Artifact(String),
}

#[derive(Debug, Clone)]
pub enum ProgramInline {
    Source(String),
    Program(ProgramFile),
}

pub type ProgramInputRef = InputRef<ProgramInline>;
pub type StateInputRef = InputRef<StateFile>;
pub type BatchInputRef = InputRef<BatchFile>;

#[derive(Debug, Clone, Copy)]
pub enum CapabilityInputMode {
    Inline,
    File,
    Artifact,
}

#[derive(Debug, Clone, Copy)]
pub enum CapabilityClientKind {
    WebIde,
    Cli,
    Automation,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub service_role: &'static str,
    pub clients: Vec<CapabilityClientKind>,
    pub compile: bool,
    pub check: bool,
    pub execute: bool,
    pub prove: bool,
    pub verify: bool,
    pub input_modes: Vec<CapabilityInputMode>,
}

#[derive(Debug, Clone)]
pub struct CheckCommand {
    pub program: ProgramInputRef,
}

#[derive(Debug, Clone)]
pub struct CompileCommand {
    pub program: ProgramInputRef,
}

#[derive(Debug, Clone)]
pub struct ExecuteCommand {
    pub program: ProgramInputRef,
    pub state: StateInputRef,
    pub batch: BatchInputRef,
    pub include_trace: bool,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub table_count: usize,
    pub tx_type_count: usize,
}

#[derive(Debug, Clone)]
pub struct CompileResult {
    pub table_count: usize,
    pub tx_type_count: usize,
    pub program: ProgramFile,
}

#[derive(Debug, Clone)]
pub struct ExecuteResult {
    pub tx_outcomes: Vec<TxOutcome>,
    pub read_set: Vec<StateCell>,
    pub write_set: Vec<StateCell>,
    pub emitted: Vec<EmittedEvent>,
    pub consistency: ExecutionConsistencyStatus,
    pub trace: Option<Vec<ExecutionEvent>>,
    pub state_after: StateFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    pub tx_types: Vec<TxTypeDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_metadata: Option<ContractMetadataEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    pub cells: Vec<StateCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateCell {
    pub table: u32,
    pub row: u64,
    pub col: u16,
    pub value: Option<Value>,
}

impl StateCell {
    pub fn to_cell_pair(&self) -> Result<(CellKey, Value), String> {
        let key = CellKey {
            table: TableId(self.table),
            col: ColId(self.col),
            row: RowKey(self.row),
        };
        let Some(value) = self.value else {
            return Err(format!(
                "state cell is missing value (table={}, row={}, col={})",
                self.table, self.row, self.col
            ));
        };
        Ok((key, value))
    }

    pub fn from_cell_pair(key: &CellKey, value: &Option<Value>) -> Self {
        Self {
            table: key.table.0,
            row: key.row.0,
            col: key.col.0,
            value: *value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchFile {
    pub transactions: Vec<TxInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TxInput {
    pub tx_type: u32,
    pub params: Vec<Value>,
    pub sender: String,
    pub nonce: u64,
}

impl TxInput {
    pub fn to_transaction(&self) -> Result<Transaction, String> {
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

fn parse_hex_32(s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Ok([0u8; 32]);
    }

    let padded = format!("{:0>64}", s);
    if padded.len() != 64 {
        return Err(format!(
            "hex string too long: expected at most 64 hex chars, got {}",
            s.len()
        ));
    }

    let mut out = [0u8; 32];
    for (i, chunk) in padded.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        out[i] = u8::from_str_radix(byte_str, 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
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
}
