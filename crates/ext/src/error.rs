use thiserror::Error;

/// Result type for extension authoring operations.
pub type ExtResult<T> = Result<T, ExtError>;

/// Errors raised while constructing or executing Tabula extensions.
#[derive(Debug, Error)]
pub enum ExtError {
    /// Authoring-time validation failed.
    #[error("validation: {detail}")]
    Validation {
        /// Description of the validation failure.
        detail: String,
    },

    /// Backend setup for an extension failed.
    #[cfg(feature = "verify")]
    #[error("setup: {0}")]
    Setup(#[source] tabula_machine::SetupError),

    /// Runtime-facing extension logic failed.
    #[error("runtime hook: {0}")]
    RuntimeHook(#[source] tabula_core::error::TabulaError),

    /// Proof-preparation logic failed.
    #[error("proof preparation: {0}")]
    ProofPreparation(#[source] tabula_core::error::TabulaError),
}

impl ExtError {
    /// Construct a validation error with a human-readable description.
    pub fn validation(detail: impl Into<String>) -> Self {
        Self::Validation {
            detail: detail.into(),
        }
    }

    /// Wrap a runtime-facing hook failure.
    pub fn runtime_hook(source: tabula_core::error::TabulaError) -> Self {
        Self::RuntimeHook(source)
    }

    /// Wrap a proof-preparation failure.
    pub fn proof_preparation(source: tabula_core::error::TabulaError) -> Self {
        Self::ProofPreparation(source)
    }
}

#[cfg(feature = "verify")]
impl From<tabula_machine::SetupError> for ExtError {
    fn from(source: tabula_machine::SetupError) -> Self {
        Self::Setup(source)
    }
}
