//! Runtime error types.

use thiserror::Error;

/// Result type for runtime operations.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Errors arising from batch execution in the runtime pipeline.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// State input is invalid or cannot be normalized.
    #[error("invalid state: {0}")]
    InvalidState(#[source] tabula_artifact::ArtifactError),

    /// Batch input is invalid or cannot be converted.
    #[error("invalid batch: {0}")]
    InvalidBatch(#[source] tabula_artifact::ArtifactError),

    /// Execution failed during batch processing.
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

    /// Construction-time validation failed.
    #[cfg(feature = "prove")]
    #[error("validation: {detail}")]
    ValidationFailed {
        /// Description of the validation failure.
        detail: String,
    },

    /// Machine setup failed (e.g., invalid column config).
    #[cfg(feature = "prove")]
    #[error("machine setup: {0}")]
    MachineSetup(#[source] tabula_machine::SetupError),

    /// Column state construction failed.
    #[cfg(feature = "prove")]
    #[error("column state: {detail}")]
    ColumnState {
        /// Description of the column state failure.
        detail: String,
    },

    /// Witness generation failed.
    #[cfg(feature = "prove")]
    #[error("witness generation: {detail}")]
    WitnessGeneration {
        /// Description of the witness generation failure.
        detail: String,
    },

    /// Trace building failed.
    #[cfg(feature = "prove")]
    #[error("trace build: {0}")]
    TraceBuild(#[source] tabula_core::error::TabulaError),

    /// STARK proving failed.
    #[cfg(feature = "prove")]
    #[error("proving: {0}")]
    Proving(#[source] tabula_machine::ProveError),

    /// STARK verification failed.
    #[cfg(feature = "prove")]
    #[error("verification: {0}")]
    Verification(#[source] tabula_machine::VerificationError),
}
