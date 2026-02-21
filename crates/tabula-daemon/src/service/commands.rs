//! Command types for service operations.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use tabula_artifact::{BatchFile, ProgramArtifact, StateFile};

// ---------------------------------------------------------------------------
// Input references
// ---------------------------------------------------------------------------

/// Generic input reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputRef<T> {
    /// Inline payload.
    Inline {
        /// Inline payload body.
        inline: T,
    },
    /// File path payload.
    File {
        /// Input file path.
        file_path: PathBuf,
    },
    /// Registry artifact reference.
    Artifact {
        /// Artifact identifier.
        artifact_id: String,
    },
}

/// Program inline payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgramInline {
    /// DSL source string.
    Source {
        /// Source code body.
        source: String,
    },
    /// Compiled program artifact.
    Program(ProgramArtifact),
}

/// Program input reference type alias.
pub type ProgramInputRef = InputRef<ProgramInline>;
/// State input reference type alias.
pub type StateInputRef = InputRef<StateFile>;
/// Batch input reference type alias.
pub type BatchInputRef = InputRef<BatchFile>;

impl<T> InputRef<T> {
    /// Build an inline input reference.
    pub fn inline(inline: T) -> Self {
        Self::Inline { inline }
    }

    /// Build a file-backed input reference.
    pub fn file(file_path: impl Into<PathBuf>) -> Self {
        Self::File {
            file_path: file_path.into(),
        }
    }

    /// Build an artifact-backed input reference.
    pub fn artifact(artifact_id: impl Into<String>) -> Self {
        Self::Artifact {
            artifact_id: artifact_id.into(),
        }
    }
}

impl ProgramInline {
    /// Build an inline source program payload.
    pub fn source(source: impl Into<String>) -> Self {
        Self::Source {
            source: source.into(),
        }
    }

    /// Build an inline compiled-program payload.
    pub fn program(program: ProgramArtifact) -> Self {
        Self::Program(program)
    }
}

// ---------------------------------------------------------------------------
// Command types
// ---------------------------------------------------------------------------

fn bool_true() -> bool {
    true
}

/// Register program command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterProgramCommand {
    /// Program input.
    pub program: ProgramInputRef,
    /// Optional user label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Fetch registered program command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProgramCommand {
    /// Program id.
    pub program_id: String,
}

/// List programs command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListProgramsCommand {}

/// Create state instance command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInstanceCommand {
    /// Program id.
    pub program_id: String,
    /// Initial state.
    pub state: StateInputRef,
    /// Optional user label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Fetch instance command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetInstanceCommand {
    /// Instance id.
    pub instance_id: String,
}

/// List instances command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListInstancesCommand {
    /// Optional program filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
}

/// Submit run command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRunCommand {
    /// Target instance id.
    pub instance_id: String,
    /// Batch input.
    pub batch: BatchInputRef,
    /// Include execution trace.
    #[serde(default)]
    pub include_trace: bool,
    /// Include proof payload.
    #[serde(default)]
    pub prove: bool,
    /// Verify proof and transition run status.
    #[serde(default)]
    pub verify: bool,
    /// Commit resulting state into instance.
    #[serde(default = "bool_true")]
    pub commit: bool,
    /// Optional optimistic-lock state version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_instance_version: Option<u64>,
}

/// Fetch run command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRunCommand {
    /// Run id.
    pub run_id: String,
}

/// List runs command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListRunsCommand {
    /// Optional instance filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Optional result limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Verify run proof command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRunCommand {
    /// Run id.
    pub run_id: String,
}
