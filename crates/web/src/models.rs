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
// flattened.  The inner record types come from `tabula-artifact`
// (canonical, single source of truth).

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
pub struct VerifyRunResponse {
    pub ok: bool,
    pub run: RunRecord,
    pub verified: bool,
    pub message: String,
    pub statement_hash: String,
}

// ── Re-exports from tabula-artifact (canonical record types) ─────────

pub type ProgramRecord = tabula_artifact::ProgramRecord;
pub type InstanceRecord = tabula_artifact::InstanceRecord;
pub type RunRecord = tabula_artifact::RunRecord;
pub type StateFile = tabula_artifact::StateFile;
pub type StateCell = tabula_artifact::StateCell;
pub type BatchFile = tabula_artifact::BatchFile;
pub type TxInput = tabula_artifact::TxInput;
pub type StarkProofSummary = tabula_artifact::StarkProofSummary;
pub type ProgramArtifact = tabula_artifact::ProgramArtifact;

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
