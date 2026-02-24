use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct DaemonErrorEnvelope {
    pub ok: bool,
    pub error: DaemonErrorPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: String,
    pub service: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesResponse {
    pub ok: bool,
    pub service_role: String,
    pub clients: Vec<String>,
    pub register_program: bool,
    pub create_instance: bool,
    pub submit_run: bool,
    pub prove: bool,
    pub verify: bool,
    pub list_programs: bool,
    pub list_instances: bool,
    pub run_history: bool,
    pub input_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRecord {
    pub program_id: String,
    pub table_count: usize,
    pub tx_type_count: usize,
    pub program: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterProgramResponse {
    pub ok: bool,
    pub program: ProgramRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstanceRecord {
    pub instance_id: String,
    pub program_id: String,
    pub version: u64,
    pub state_hash: String,
    pub state: StateFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstanceResponse {
    pub ok: bool,
    pub instance: CreateInstanceRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct RunRecordResponse {
    pub run_id: String,
    pub status: String,
    pub execution: ExecutePayload,
    pub proof: Option<ExecutionReceipt>,
    pub stark_proof: Option<StarkProofSummary>,
    pub statement_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRunResponse {
    pub ok: bool,
    pub run: RunRecordResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRunResponse {
    pub ok: bool,
    pub run: RunRecordResponse,
    pub verified: bool,
    pub message: String,
    pub statement_hash: String,
}

pub type StateFile = tabula_artifact::StateFile;
pub type StateCell = tabula_artifact::StateCell;
pub type BatchFile = tabula_artifact::BatchFile;
pub type TxInput = tabula_artifact::TxInput;
pub type ExecutionReceipt = tabula_artifact::ExecutionReceipt;
pub type StarkProofSummary = tabula_artifact::StarkProofSummary;
pub type ProgramArtifact = tabula_artifact::ProgramArtifact;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub ts_ms: f64,
    pub action: String,
    pub ok: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyReport {
    pub ok: bool,
    pub mode: String,
    pub message: String,
    pub statement_hash: Option<String>,
    pub expected_statement_hash: Option<String>,
    pub checked_at_ms: f64,
    pub raw: Option<Value>,
}
