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

// ── Daemon error envelope ────────────────────────────────────────────

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

// ── Daemon response envelopes ────────────────────────────────────────
//
// The daemon wraps every response in `{ ok: true, ...T }` where T is
// flattened. The inner control-plane record types are daemon-specific.

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
pub struct RegisterProgramResponse {
    pub ok: bool,
    pub program: ProgramRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstanceResponse {
    pub ok: bool,
    pub instance: InstanceRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRunResponse {
    pub ok: bool,
    pub run: RunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub struct VerifyRunResponse {
    pub ok: bool,
    pub run: RunRecord,
    pub verified: bool,
    pub message: String,
    pub statement_hash: String,
}

// ── Shared canonical artifact types ──────────────────────────────────

pub type StateSnapshot = tabula_artifact::StateSnapshot;
pub type StateEntry = tabula_artifact::StateEntry;
pub type TransactionBatch = tabula_artifact::TransactionBatch;
pub type TransactionInput = tabula_artifact::TransactionInput;
pub type ProgramArtifact = tabula_artifact::ProgramArtifact;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct ChipSummary {
    pub name: String,
    pub trace_height: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarkProofSummary {
    pub scheme: String,
    pub verified: bool,
    pub chip_count: usize,
    pub chips: Vec<ChipSummary>,
    pub old_state_root: Vec<String>,
    pub new_state_root: Vec<String>,
    pub prove_time_ms: u64,
    pub verify_time_ms: u64,
    pub statement_hash: String,
    pub program_hash: String,
    pub batch_hash: String,
}

// ── Daemon control-plane record types ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRecord {
    pub program_id: String,
    pub label: Option<String>,
    pub created_at_ms: u64,
    pub table_count: usize,
    pub tx_type_count: usize,
    pub profile_hash: String,
    pub metadata_hash: String,
    pub program_hash: String,
    pub contract_schema_version: u32,
    pub binding_version: u32,
    pub statement_schema_version: u32,
    pub verifier_profile_version: u32,
    pub program: ProgramArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceRecord {
    pub instance_id: String,
    pub program_id: String,
    pub label: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub version: u64,
    pub status: String,
    pub state_hash: String,
    pub state: StateSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSummary {
    #[serde(rename = "tx_results")]
    pub tx_outcomes: Vec<Value>,
    pub read_set: Vec<StateEntry>,
    pub write_set: Vec<StateEntry>,
    pub emitted: Vec<Value>,
    pub consistency: Value,
    pub trace: Option<Vec<Value>>,
    pub state_after: StateSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub run_id: String,
    pub program_id: String,
    pub instance_id: String,
    pub created_at_ms: u64,
    pub status: String,
    pub committed: bool,
    pub include_trace: bool,
    pub prove: bool,
    pub verify: bool,
    pub instance_version_before: u64,
    pub instance_version_after: u64,
    pub state_hash_before: String,
    pub state_hash_after: String,
    pub program_hash: String,
    pub batch_hash: String,
    pub metadata_hash: String,
    pub statement_hash: String,
    pub execution: ExecutionSummary,
    pub proof: Option<ExecutionReceipt>,
    pub stark_proof: Option<StarkProofSummary>,
    pub proof_verified: Option<bool>,
    pub verification_message: Option<String>,
    pub verified_at_ms: Option<u64>,
}

// ── Client-local types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunHistoryEntry {
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
