//! Shared domain record types for programs, instances, and runs.

use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

use tabula_core::{EmittedEvent, ExecutionConsistencyStatus, ExecutionEvent, TxOutcome};

use crate::{ExecutionReceipt, ProgramArtifact, StarkProofSummary, StateCell, StateFile};

// ---------------------------------------------------------------------------
// Newtype identifiers
// ---------------------------------------------------------------------------

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create a new identifier.
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// View as string slice.
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

/// Execution summary payload for one run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSummary {
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
    pub execution: ExecutionSummary,
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
