//! Runtime error types.
//!
//! `RuntimeError` is a thin umbrella over four narrowed error families
//! that track the runtime pipeline phases (setup, prove, verify, execute).
//! Constructors should build the narrowed variant directly and rely on
//! `#[from]` / `?` to widen at the `pub` boundary.

use thiserror::Error;

/// Result type for runtime operations.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Top-level runtime error. Each variant transparently wraps the
/// phase-specific narrowed error so callers can discriminate by phase
/// without caring about the full variant list.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    /// Setup / preparation failure (bootstrap, snapshot construction,
    /// host registration, runtime assembly).
    #[error(transparent)]
    Setup(#[from] SetupError),
    /// Proving failure.
    #[cfg(feature = "prove")]
    #[error(transparent)]
    Prove(#[from] ProveError),
    /// Verification failure (pre / post proof check, statement build).
    #[cfg(feature = "verify")]
    #[error(transparent)]
    Verify(#[from] VerifyError),
    /// Batch / query execution failure.
    #[cfg(feature = "verify")]
    #[error(transparent)]
    Execute(#[from] ExecuteError),
}

/// Errors observed during runtime setup and preparation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetupError {
    /// Construction-time validation failed.
    #[error("validation: {detail}")]
    Validation {
        /// Description of the validation failure.
        detail: String,
    },
    /// Machine setup failed (e.g., invalid column config).
    #[cfg(feature = "verify")]
    #[error("machine setup: {0}")]
    MachineSetup(#[source] tabula_machine::SetupError),
    /// Program artifact failed compiler-side semantic validation.
    #[error("compiler validation: {0}")]
    CompilerValidation(#[source] tabula_compiler::CompilerError),
    /// Extension-reported setup failure.
    #[error("extension setup: {0}")]
    Extension(#[source] tabula_ext::ExtError),
}

/// Errors observed during proving.
#[cfg(feature = "prove")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProveError {
    /// Witness generation failed.
    #[error("witness generation: {detail}")]
    WitnessGeneration {
        /// Description of the witness generation failure.
        detail: String,
    },
    /// Trace building failed.
    #[error("trace build: {0}")]
    TraceBuild(#[source] tabula_core::error::TabulaError),
    /// Column state construction failed.
    #[error("commitment state: {detail}")]
    CommitmentState {
        /// Description of the commitment state failure.
        detail: String,
    },
    /// STARK proving failed.
    #[error("proving: {0}")]
    Proving(#[source] tabula_machine::ProveError),
    /// Post-prove verification failed (observed by `prove_and_verify`).
    ///
    /// Wraps the full [`VerifyError`] so every verify-side variant
    /// (`Verification`, `StatementBuild`, `Validation`, `Setup`) surfaces
    /// with its source chain preserved.
    #[error("post-prove verify: {0}")]
    PostVerify(#[source] VerifyError),
    /// Setup-phase failure observed on the prover surface
    /// (handle construction, pre-prove validation).
    #[error("setup: {0}")]
    Setup(#[source] SetupError),
    /// Pre-prove verify-side failure observed on the prover surface
    /// (statement build, verifier-side validation during proof preparation).
    #[error("verify: {0}")]
    Verify(#[source] VerifyError),
    /// Execute-phase failure observed on the prover surface
    /// (batch / query decode steps during proof preparation).
    #[error("execute: {0}")]
    Execute(#[source] ExecuteError),
}

/// Errors observed during verification and public statement assembly.
#[cfg(feature = "verify")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// STARK verification failed.
    #[error("verification: {0}")]
    Verification(#[source] tabula_machine::VerificationError),
    /// Canonical execution statement construction failed.
    #[error("statement build: {detail}")]
    StatementBuild {
        /// Description of the statement construction failure.
        detail: String,
    },
    /// Verifier-side validation failed.
    #[error("validation: {detail}")]
    Validation {
        /// Description of the validation failure.
        detail: String,
    },
    /// Setup-phase failure observed on the verifier surface
    /// (sealed artifact resolution, machine build during handle construction).
    #[error("setup: {0}")]
    Setup(#[source] SetupError),
}

/// Errors observed during batch / query execution.
#[cfg(feature = "verify")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecuteError {
    /// Execution failed during batch processing.
    #[error("execution failed")]
    Execution {
        /// Underlying execution error.
        #[source]
        source: tabula_core::error::TabulaError,
        /// Index of the instruction that failed (if available).
        instruction_index: Option<usize>,
        /// Index of the transaction within the batch (if available).
        tx_index: Option<u32>,
    },
    /// Post-execution / batch-path validation failed.
    #[error("validation: {detail}")]
    Validation {
        /// Description of the validation failure.
        detail: String,
    },
    /// Setup-phase failure observed on the executor surface
    /// (handle construction, snapshot validation during batch dispatch).
    #[error("setup: {0}")]
    Setup(#[source] SetupError),
}
