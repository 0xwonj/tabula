use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDoc {
    pub daemon_url: String,
    pub auth_token: String,
    pub program_source: String,
    pub state_json: String,
    pub batch_json: String,
    pub include_trace: bool,
    pub proof_json: String,
    pub verify_result_json: String,
}

impl WorkspaceDoc {
    pub fn defaults() -> Self {
        Self {
            daemon_url: "http://127.0.0.1:4317".to_string(),
            auth_token: String::new(),
            program_source: String::new(),
            state_json: String::new(),
            batch_json: String::new(),
            include_trace: true,
            proof_json: String::new(),
            verify_result_json: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonErrorEnvelope {
    pub ok: bool,
    pub error: DaemonErrorPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: String,
    pub service: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesResponse {
    pub ok: bool,
    pub service_role: String,
    pub clients: Vec<String>,
    pub compile: bool,
    pub check: bool,
    pub execute: bool,
    pub prove: bool,
    pub verify: bool,
    pub input_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub ok: bool,
    pub table_count: usize,
    pub tx_type_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResponse {
    pub ok: bool,
    pub table_count: usize,
    pub tx_type_count: usize,
    pub program: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub ok: bool,
    pub tx_outcomes: Vec<Value>,
    pub read_set: Vec<StateCell>,
    pub write_set: Vec<StateCell>,
    pub emitted: Vec<Value>,
    pub consistency: Value,
    pub trace: Option<Vec<Value>>,
    pub state_after: StateFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutePayload {
    pub tx_outcomes: Vec<Value>,
    pub read_set: Vec<StateCell>,
    pub write_set: Vec<StateCell>,
    pub emitted: Vec<Value>,
    pub consistency: Value,
    pub trace: Option<Vec<Value>>,
    pub state_after: StateFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub version: u32,
    pub scheme: String,
    pub statement_hash: String,
    pub program_hash: String,
    pub state_hash: String,
    pub batch_hash: String,
    pub state_after_hash: String,
    pub metadata_hash: String,
    pub generated_at_ms: u64,
    pub tx_count: usize,
    pub emitted_count: usize,
    pub consistency: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveResponse {
    pub ok: bool,
    pub proof: ExecutionReceipt,
    pub execution: ExecutePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub ok: bool,
    pub verified: bool,
    pub message: String,
    pub statement_hash: Option<String>,
    pub expected_statement_hash: Option<String>,
    pub matched_expected: Option<bool>,
    pub proof: Option<ExecutionReceipt>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub ts_ms: f64,
    pub action: String,
    pub ok: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub mode: String,
    pub message: String,
    pub statement_hash: Option<String>,
    pub expected_statement_hash: Option<String>,
    pub checked_at_ms: f64,
    pub raw: Option<Value>,
}
