use thiserror::Error;

/// Errors returned by the Tabula SDK facade.
#[derive(Debug, Error)]
pub enum SdkError {
    /// Compiler or artifact-registration failure.
    #[error(transparent)]
    Compiler(#[from] tabula_compiler::CompilerError),
    /// Artifact normalization or canonicalization failure.
    #[error(transparent)]
    Artifact(#[from] tabula_artifact::ArtifactError),
    /// Runtime execution, setup, prove, or verify failure.
    #[error(transparent)]
    Runtime(#[from] tabula_runtime::RuntimeError),
    /// Invalid or conflicting semantic registry configuration.
    #[error("invalid semantic registry: {detail}")]
    InvalidSemanticRegistry {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Invalid or conflicting custom column backend registration.
    #[error("invalid column backend bundle: {detail}")]
    InvalidColumnBackendBundle {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Invalid or conflicting precompile descriptor registration.
    #[error("invalid precompile descriptor registration: {detail}")]
    InvalidPrecompileDescriptorRegistration {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Invalid or conflicting installed precompile backend registration.
    #[error("invalid precompile backend bundle: {detail}")]
    InvalidPrecompileBackendBundle {
        /// Human-readable validation detail.
        detail: String,
    },
    /// The requested operation needs an SDK feature that is not enabled.
    #[error("feature `{feature}` is required: {detail}")]
    FeatureDisabled {
        /// Missing cargo feature.
        feature: &'static str,
        /// Human-readable guidance.
        detail: String,
    },
    /// The provided execution belongs to a different program artifact.
    #[error("execution does not belong to this program")]
    ExecutionProgramMismatch,
}
