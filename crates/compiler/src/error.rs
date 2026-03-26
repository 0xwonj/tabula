//! Compiler error types.

use serde::{Deserialize, Serialize};
use tabula_profile::ProfileError;
use thiserror::Error;

/// Compiler result type.
pub type CompilerResult<T> = Result<T, CompilerError>;

/// Structured compile diagnostic for adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDiagnostic {
    /// Pipeline stage that produced this diagnostic.
    pub stage: CompileStage,
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

/// Compiler pipeline stage used for structured diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompileStage {
    /// Source-semantic context construction and validation.
    FrontendSemantics,
    /// Source parser.
    FrontendParse,
    /// HIR builder.
    FrontendBuild,
    /// HIR verifier.
    FrontendVerify,
    /// HIR -> MIR lowering.
    HirLower,
    /// MIR structural verification.
    MirVerify,
    /// MIR analysis.
    MirAnalyze,
    /// MIR normalization/inlining.
    MirNormalize,
    /// MIR -> canonical lowering.
    MirLower,
    /// Canonical validation.
    CanonicalValidate,
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
    /// Sealed artifact diverges from the compiler-derived canonical shape.
    #[error("sealed artifact mismatch: {detail}")]
    ArtifactMismatch {
        /// Human-readable mismatch detail.
        detail: String,
    },
    /// Compiled artifact metadata mismatched current semantic policy.
    #[error("contract metadata mismatch: {0}")]
    ContractMetadataMismatch(#[source] tabula_contract::ContractValidationError),
}

/// Errors returned while mutating compiler-owned sealing catalogs.
#[derive(Debug, Error)]
pub enum CompilerCatalogError {
    /// Semantic registry failed validation.
    #[error("invalid semantic registry: {0}")]
    InvalidSemanticRegistry(#[source] ProfileError),
    /// Duplicate source capability descriptors are not allowed.
    #[error("duplicate capability descriptor registration for path {path}")]
    DuplicateCapabilityDescriptor {
        /// Conflicting capability import path.
        path: String,
    },
    /// Source capability descriptor contract is invalid for the active semantic registry.
    #[error("invalid capability descriptor registration: {detail}")]
    InvalidCapabilityDescriptor {
        /// Human-readable validation detail.
        detail: String,
    },
}
