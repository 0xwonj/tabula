//! Driver error types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Driver result type.
pub type DriverResult<T> = Result<T, DriverError>;

/// Structured compile diagnostic for adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDiagnostic {
    /// Compile error kind.
    pub kind: String,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Byte span start.
    pub span_start: usize,
    /// Byte span end.
    pub span_end: usize,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
}

/// Driver-level error type shared across adapters/orchestration.
#[derive(Debug, Error)]
pub enum DriverError {
    /// Program source read failed.
    #[error("failed to read {path}: {source}")]
    ReadFile {
        /// File path.
        path: String,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// Program JSON parse failed.
    #[error("failed to parse {path}: {source}")]
    ParseJson {
        /// File path or logical label.
        path: String,
        /// Source error.
        #[source]
        source: serde_json::Error,
    },
    /// Program compile failed.
    #[error("program compilation failed")]
    Compile {
        /// Structured diagnostics.
        diagnostics: Vec<CompileDiagnostic>,
    },
    /// Program failed semantic registration.
    #[error("invalid program: {0}")]
    InvalidProgram(#[source] anyhow::Error),
    /// Compiled artifact is missing contract metadata.
    #[error(
        "compiled program JSON is missing contract_metadata; regenerate with the current driver"
    )]
    MissingContractMetadata,
    /// Compiled artifact metadata mismatched current semantic policy.
    #[error("contract metadata mismatch: {0}")]
    ContractMetadataMismatch(#[source] tabula_contract::ContractValidationError),
    /// State input is invalid.
    #[error("invalid state: {0}")]
    InvalidState(#[source] tabula_artifact::ArtifactError),
    /// Batch input is invalid.
    #[error("invalid batch: {0}")]
    InvalidBatch(#[source] tabula_artifact::ArtifactError),
    /// Execution failed.
    #[error("execution failed: {source}")]
    Execution {
        /// Underlying execution error.
        #[source]
        source: tabula_core::error::TabulaError,
        /// Index of the instruction that failed (if available).
        instruction_index: Option<usize>,
        /// Index of the transaction within the batch (if available).
        tx_index: Option<u32>,
    },
}

impl From<tabula_runtime::RuntimeError> for DriverError {
    fn from(err: tabula_runtime::RuntimeError) -> Self {
        match err {
            tabula_runtime::RuntimeError::InvalidState(e) => Self::InvalidState(e),
            tabula_runtime::RuntimeError::InvalidBatch(e) => Self::InvalidBatch(e),
            tabula_runtime::RuntimeError::Execution {
                source,
                instruction_index,
                tx_index,
            } => Self::Execution {
                source,
                instruction_index,
                tx_index,
            },
            // Proving variants (feature-gated in RuntimeError) are never produced
            // by the driver's execute-only path; map to generic execution error.
            #[allow(unreachable_patterns)]
            other => Self::Execution {
                source: tabula_core::error::TabulaError::ProofError {
                    phase: "runtime",
                    detail: other.to_string(),
                },
                instruction_index: None,
                tx_index: None,
            },
        }
    }
}
