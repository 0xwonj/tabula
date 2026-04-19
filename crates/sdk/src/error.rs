use thiserror::Error;

/// Errors returned by the Tabula SDK facade.
#[derive(Debug, Error)]
pub enum SdkError {
    /// Compiler or registration failure.
    #[error(transparent)]
    Compiler(#[from] tabula_compiler::CompilerError),
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
    /// Invalid or conflicting native capability descriptor registration.
    #[error("invalid capability descriptor registration: {detail}")]
    InvalidCapabilityDescriptorRegistration {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Invalid extension bundle or installation request.
    #[error("invalid extension: {detail}")]
    InvalidExtension {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Artifact decoding failure.
    #[error("invalid artifact payload: {detail}")]
    ArtifactDecode {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Proof envelope or proof byte decoding failure.
    #[error("invalid proof payload: {detail}")]
    ProofDecode {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Schema lookup failed on the default SDK path.
    #[error("schema lookup failed: {detail}")]
    SchemaLookup {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Value encoding failed on the default SDK path.
    #[error("value encoding failed: {detail}")]
    ValueEncoding {
        /// Human-readable validation detail.
        detail: String,
    },
    /// Value decoding failed on the default SDK path.
    #[error("value decoding failed: {detail}")]
    ValueDecoding {
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
    #[error("execution receipt does not belong to this program")]
    ExecutionProgramMismatch,
    /// Internal synchronization or cache state failed.
    #[error("internal synchronization failure: {detail}")]
    Synchronization {
        /// Human-readable detail.
        detail: String,
    },
}

impl From<tabula_runtime::SetupError> for SdkError {
    fn from(error: tabula_runtime::SetupError) -> Self {
        SdkError::Runtime(tabula_runtime::RuntimeError::from(error))
    }
}

#[cfg(feature = "prove")]
impl From<tabula_runtime::ProveError> for SdkError {
    fn from(error: tabula_runtime::ProveError) -> Self {
        SdkError::Runtime(tabula_runtime::RuntimeError::from(error))
    }
}

#[cfg(feature = "verify")]
impl From<tabula_runtime::VerifyError> for SdkError {
    fn from(error: tabula_runtime::VerifyError) -> Self {
        SdkError::Runtime(tabula_runtime::RuntimeError::from(error))
    }
}

#[cfg(feature = "verify")]
impl From<tabula_runtime::ExecuteError> for SdkError {
    fn from(error: tabula_runtime::ExecuteError) -> Self {
        SdkError::Runtime(tabula_runtime::RuntimeError::from(error))
    }
}

/// Build/install errors returned by the configurable SDK path.
pub type InstallError = SdkError;
