//! Compiler error types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Compiler result type.
pub type CompilerResult<T> = Result<T, CompilerError>;

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

/// Compiler-level error type shared across adapters/orchestration.
#[derive(Debug, Error)]
pub enum CompilerError {
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
        "compiled program JSON is missing contract_metadata; regenerate with the current compiler"
    )]
    MissingContractMetadata,
    /// Compiled artifact metadata mismatched current semantic policy.
    #[error("contract metadata mismatch: {0}")]
    ContractMetadataMismatch(#[source] tabula_contract::ContractValidationError),
}
