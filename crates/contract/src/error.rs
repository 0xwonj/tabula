use crate::compatibility::ContractValidationError;

/// Error raised when encoding or decoding contract-owned proof artifacts fails.
#[derive(Debug, thiserror::Error)]
pub enum ProofContractError {
    /// Canonical statement encoding failed.
    #[error("failed to encode artifact-bound statement: {detail}")]
    StatementEncode {
        /// Human-readable detail.
        detail: String,
    },
    /// Canonical envelope encoding failed.
    #[error("failed to encode proof envelope: {detail}")]
    EnvelopeEncode {
        /// Human-readable detail.
        detail: String,
    },
    /// Canonical envelope decoding failed.
    #[error("failed to decode proof envelope: {detail}")]
    EnvelopeDecode {
        /// Human-readable detail.
        detail: String,
    },
    /// Proof envelope magic prefix is invalid.
    #[error("invalid proof envelope magic")]
    InvalidEnvelopeMagic,
    /// Unsupported proof system identifier.
    #[error("unsupported proof system id {got}")]
    UnknownProofSystemId {
        /// Identifier carried by the envelope.
        got: u16,
    },
    /// Unsupported proof encoding identifier.
    #[error("unsupported proof encoding id {got}")]
    UnknownProofEncodingId {
        /// Identifier carried by the envelope.
        got: u16,
    },
    /// Contract validation failed.
    #[error(transparent)]
    ContractValidation(#[from] ContractValidationError),
}
