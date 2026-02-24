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
    #[error("invalid program: {message}")]
    InvalidProgram {
        /// Validation error text.
        message: String,
    },
    /// Compiled artifact is missing contract metadata.
    #[error(
        "compiled program JSON is missing contract_metadata; regenerate with the current driver"
    )]
    MissingContractMetadata,
    /// Compiled artifact metadata mismatched current semantic policy.
    #[error("contract metadata mismatch: {message}")]
    ContractMetadataMismatch {
        /// Validation mismatch details.
        message: String,
    },
    /// State input is invalid.
    #[error("invalid state: {message}")]
    InvalidState {
        /// Validation error detail.
        message: String,
    },
    /// Batch input is invalid.
    #[error("invalid batch: {message}")]
    InvalidBatch {
        /// Validation error detail.
        message: String,
    },
    /// Execution failed.
    #[error("execution failed: {message}")]
    Execution {
        /// Failure detail.
        message: String,
    },
}
