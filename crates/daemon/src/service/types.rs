//! Daemon-local control-plane command and record types.
#![allow(missing_docs)]

use std::borrow::Borrow;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use tabula_artifact::{Artifact, State, StateEntry, TransactionBatch};
use tabula_core::{AccessEvent, EmittedEvent, ExecutionConsistencyStatus, TxResult};

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                self.0 == *other
            }
        }
    };
}

define_id!(
    /// Program identifier.
    ProgramId
);
define_id!(
    /// Stateful instance identifier.
    InstanceId
);
define_id!(
    /// Run identifier.
    RunId
);

/// Lifecycle status for a program-backed state instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Active,
    Archived,
}

/// Lifecycle status for a submitted run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Succeeded,
    Verified,
    VerificationFailed,
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Succeeded => f.write_str("succeeded"),
            Self::Verified => f.write_str("verified"),
            Self::VerificationFailed => f.write_str("verification_failed"),
        }
    }
}

/// Execution summary payload for one run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSummary {
    pub tx_results: Vec<TxResult>,
    pub read_set: Vec<StateEntry>,
    pub write_set: Vec<StateEntry>,
    pub emitted: Vec<EmittedEvent>,
    pub consistency: ExecutionConsistencyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<AccessEvent>>,
    pub state_after: State,
}

/// Serializable execution receipt used by daemon APIs.
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
    pub consistency: ExecutionConsistencyStatus,
}

/// Summary of one chip in a STARK proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipSummary {
    pub name: String,
    pub trace_height: usize,
}

/// Serializable STARK proof summary returned by daemon APIs.
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

/// Registered program record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRecord {
    pub program_id: ProgramId,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    pub program: Artifact,
}

/// Stateful instance record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceRecord {
    pub instance_id: InstanceId,
    pub program_id: ProgramId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub version: u64,
    pub status: InstanceStatus,
    pub state_hash: String,
    pub state: State,
}

/// Run record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub run_id: RunId,
    pub program_id: ProgramId,
    pub instance_id: InstanceId,
    pub created_at_ms: u64,
    pub status: RunStatus,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<ExecutionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stark_proof: Option<StarkProofSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at_ms: Option<u64>,
}

/// Verification outcome returned by `verify_run`.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyOutcome {
    pub run: RunRecord,
    pub verified: bool,
    pub message: String,
    pub statement_hash: String,
}

/// Generic input reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputRef<T> {
    Inline { inline: T },
    File { file_path: PathBuf },
    Artifact { artifact_id: String },
}

/// Program inline payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgramInline {
    Source { source: String },
    Program(Artifact),
}

pub type ProgramInputRef = InputRef<ProgramInline>;
pub type StateInputRef = InputRef<State>;
pub type BatchInputRef = InputRef<TransactionBatch>;

impl<T> InputRef<T> {
    pub fn inline(inline: T) -> Self {
        Self::Inline { inline }
    }

    pub fn file(file_path: impl Into<PathBuf>) -> Self {
        Self::File {
            file_path: file_path.into(),
        }
    }

    pub fn artifact(artifact_id: impl Into<String>) -> Self {
        Self::Artifact {
            artifact_id: artifact_id.into(),
        }
    }
}

impl ProgramInline {
    pub fn source(source: impl Into<String>) -> Self {
        Self::Source {
            source: source.into(),
        }
    }

    pub fn program(program: Artifact) -> Self {
        Self::Program(program)
    }
}

fn bool_true() -> bool {
    true
}

/// Register program command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterProgramCommand {
    pub program: ProgramInputRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Fetch registered program command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProgramCommand {
    pub program_id: ProgramId,
}

/// List programs command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListProgramsCommand {}

/// Create state instance command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstanceCommand {
    pub program_id: ProgramId,
    pub state: StateInputRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Fetch instance command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetInstanceCommand {
    pub instance_id: InstanceId,
}

/// List instances command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListInstancesCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_id: Option<ProgramId>,
}

/// Submit run command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRunCommand {
    pub instance_id: InstanceId,
    pub batch: BatchInputRef,
    #[serde(default)]
    pub include_trace: bool,
    #[serde(default)]
    pub prove: bool,
    #[serde(default)]
    pub verify: bool,
    #[serde(default = "bool_true")]
    pub commit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_instance_version: Option<u64>,
}

/// Fetch run command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRunCommand {
    pub run_id: RunId,
}

/// List runs command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListRunsCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<InstanceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Verify run proof command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRunCommand {
    pub run_id: RunId,
}
