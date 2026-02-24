//! Domain record types for the service layer.

use serde::{Deserialize, Serialize};

use tabula_artifact::{ProgramArtifact, StarkProofSummary, StateCell, StateFile};
use tabula_core::{EmittedEvent, ExecutionConsistencyStatus, ExecutionEvent, TxOutcome};

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Program identifier.
pub type ProgramId = String;
/// Stateful instance identifier.
pub type InstanceId = String;
/// Run identifier.
pub type RunId = String;
/// Execution receipt artifact type alias.
pub type ExecutionReceipt = tabula_artifact::ExecutionReceipt;

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Capability input modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityInputMode {
    /// Inline mode.
    Inline,
    /// File mode.
    File,
    /// Artifact mode.
    Artifact,
}

/// Supported client kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityClientKind {
    /// Web IDE client.
    WebIde,
    /// CLI client.
    Cli,
    /// Automation client.
    Automation,
}

/// Service capabilities.
#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// Service role name.
    pub service_role: &'static str,
    /// Supported clients.
    pub clients: Vec<CapabilityClientKind>,
    /// Program registration support.
    pub register_program: bool,
    /// Stateful instance creation support.
    pub create_instance: bool,
    /// Run submission support.
    pub submit_run: bool,
    /// Proof generation support during run submission.
    pub prove: bool,
    /// Proof verification support for completed runs.
    pub verify: bool,
    /// Program listing/fetch support.
    pub list_programs: bool,
    /// Instance listing/fetch support.
    pub list_instances: bool,
    /// Run listing/fetch support.
    pub run_history: bool,
    /// Supported input modes.
    pub input_modes: Vec<CapabilityInputMode>,
}

// ---------------------------------------------------------------------------
// Lifecycle enums
// ---------------------------------------------------------------------------

/// Lifecycle status for a program-backed state instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    /// Instance can accept new runs.
    Active,
    /// Instance is archived and cannot accept new runs.
    Archived,
}

/// Lifecycle status for a submitted run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run completed successfully.
    Succeeded,
    /// Run proof has been verified.
    Verified,
    /// Run proof verification failed.
    VerificationFailed,
}

// ---------------------------------------------------------------------------
// Record types
// ---------------------------------------------------------------------------

/// Registered program record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRecord {
    /// Program id.
    pub program_id: ProgramId,
    /// Optional user label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Creation timestamp.
    pub created_at_ms: u64,
    /// Number of tables.
    pub table_count: usize,
    /// Number of tx types.
    pub tx_type_count: usize,
    /// Driver semantic profile hash.
    pub profile_hash: String,
    /// Contract metadata hash.
    pub metadata_hash: String,
    /// Program artifact hash.
    pub program_hash: String,
    /// Canonical program artifact.
    pub program: ProgramArtifact,
}

/// Stateful instance record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceRecord {
    /// Instance id.
    pub instance_id: InstanceId,
    /// Program id.
    pub program_id: ProgramId,
    /// Optional user label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Creation timestamp.
    pub created_at_ms: u64,
    /// Last-update timestamp.
    pub updated_at_ms: u64,
    /// Monotonic state version.
    pub version: u64,
    /// Current lifecycle status.
    pub status: InstanceStatus,
    /// Current state hash.
    pub state_hash: String,
    /// Current full state.
    pub state: StateFile,
}

/// Execution result payload for one run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResult {
    /// Per-tx outcomes.
    pub tx_outcomes: Vec<TxOutcome>,
    /// Read set.
    pub read_set: Vec<StateCell>,
    /// Write set.
    pub write_set: Vec<StateCell>,
    /// Emitted events.
    pub emitted: Vec<EmittedEvent>,
    /// Consistency status.
    pub consistency: ExecutionConsistencyStatus,
    /// Optional full trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<ExecutionEvent>>,
    /// Post-state.
    pub state_after: StateFile,
}

/// Run record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    /// Run id.
    pub run_id: RunId,
    /// Program id.
    pub program_id: ProgramId,
    /// Instance id.
    pub instance_id: InstanceId,
    /// Creation timestamp.
    pub created_at_ms: u64,
    /// Run lifecycle status.
    pub status: RunStatus,
    /// Whether state was committed.
    pub committed: bool,
    /// Whether trace was included in execution payload.
    pub include_trace: bool,
    /// Whether proof generation was requested.
    pub prove: bool,
    /// Whether proof verification was requested.
    pub verify: bool,
    /// Instance version before execution.
    pub instance_version_before: u64,
    /// Instance version after execution.
    pub instance_version_after: u64,
    /// State hash before execution.
    pub state_hash_before: String,
    /// State hash after execution.
    pub state_hash_after: String,
    /// Program hash for the statement.
    pub program_hash: String,
    /// Batch hash for the statement.
    pub batch_hash: String,
    /// Metadata hash for the statement.
    pub metadata_hash: String,
    /// Statement hash for the run.
    pub statement_hash: String,
    /// Execution payload.
    pub execution: ExecutionResult,
    /// Optional proof payload (non-STARK receipt).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<ExecutionReceipt>,
    /// Optional STARK proof summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stark_proof: Option<StarkProofSummary>,
    /// Latest proof verification result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_verified: Option<bool>,
    /// Proof verification message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_message: Option<String>,
    /// Proof verification timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at_ms: Option<u64>,
}

/// Verification outcome returned by verify_run.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyOutcome {
    /// Updated run record.
    pub run: RunRecord,
    /// Verification success.
    pub verified: bool,
    /// Verification message.
    pub message: String,
    /// Run statement hash.
    pub statement_hash: String,
}
