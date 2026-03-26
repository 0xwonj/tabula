//! Runtime error types.

use thiserror::Error;

/// Result type for runtime operations.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Errors arising from batch execution in the runtime pipeline.
#[derive(Debug, Error)]
pub enum RuntimeError {
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
    #[error("validation: {detail}")]
    ValidationFailed {
        /// Description of the validation failure.
        detail: String,
    },

    /// Program artifact failed compiler-side semantic validation.
    #[cfg(feature = "verify")]
    #[error("compiler validation: {0}")]
    CompilerValidation(#[source] tabula_compiler::CompilerError),

    /// Machine setup failed (e.g., invalid column config).
    #[cfg(feature = "verify")]
    #[error("machine setup: {0}")]
    MachineSetup(#[source] tabula_machine::SetupError),

    /// Column state construction failed.
    #[cfg(feature = "prove")]
    #[error("commitment state: {detail}")]
    CommitmentState {
        /// Description of the commitment state failure.
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

    /// Canonical execution statement construction failed.
    #[cfg(feature = "verify")]
    #[error("statement build: {detail}")]
    StatementBuild {
        /// Description of the statement construction failure.
        detail: String,
    },

    /// STARK proving failed.
    #[cfg(feature = "prove")]
    #[error("proving: {0}")]
    Proving(#[source] tabula_machine::ProveError),

    /// STARK verification failed.
    #[cfg(feature = "verify")]
    #[error("verification: {0}")]
    Verification(#[source] tabula_machine::VerificationError),
}

impl RuntimeError {
    pub(crate) fn from_extension_setup(error: tabula_ext::ExtError) -> Self {
        match error {
            tabula_ext::ExtError::Validation { detail } => Self::ValidationFailed { detail },
            #[cfg(feature = "verify")]
            tabula_ext::ExtError::Setup(source) => Self::MachineSetup(source),
            tabula_ext::ExtError::RuntimeHook(source)
            | tabula_ext::ExtError::ProofPreparation(source) => Self::ValidationFailed {
                detail: source.to_string(),
            },
        }
    }

    #[cfg(feature = "prove")]
    pub(crate) fn from_extension_proof(error: tabula_ext::ExtError) -> Self {
        match error {
            tabula_ext::ExtError::Validation { detail } => Self::WitnessGeneration { detail },
            #[cfg(feature = "verify")]
            tabula_ext::ExtError::Setup(source) => Self::MachineSetup(source),
            tabula_ext::ExtError::RuntimeHook(source)
            | tabula_ext::ExtError::ProofPreparation(source) => Self::WitnessGeneration {
                detail: source.to_string(),
            },
        }
    }
}

impl From<tabula_ext::ExtError> for RuntimeError {
    fn from(value: tabula_ext::ExtError) -> Self {
        Self::from_extension_setup(value)
    }
}
