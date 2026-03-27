//! Contract-owned proof-visible statements and proof envelope schema.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tabula_core::{Digest, PortableValue};
use tabula_ir as ir;

use crate::binding::ProgramBinding;
use crate::compatibility::ContractValidationError;
use crate::versions::{
    PROOF_ENVELOPE_VERSION, STATEMENT_SCHEMA_VERSION, validate_proof_envelope_version,
    validate_statement_schema_version,
};

const PROOF_STATEMENT_DOMAIN: &[u8] = b"tabula.contract.proof_statement";
const PROOF_ENVELOPE_MAGIC: &[u8] = b"tabula.contract.proof";

/// Error raised when encoding or decoding contract-owned proof artifacts fails.
#[derive(Debug, thiserror::Error)]
pub enum ProofContractError {
    /// Canonical statement encoding failed.
    #[error("failed to encode proof statement: {detail}")]
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

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CanonicalProofStatement {
    schema_version: u32,
    program_hash: Digest,
    metadata_hash: Digest,
    program_id: ir::ProgramId,
    public_context: Vec<PublicContextBinding>,
    event_digest: Digest,
    applied_tx_digest: Digest,
    static_table_root: Digest,
    old_state_root: Digest,
    new_state_root: Digest,
}

/// A portable binding of one public context field to its committed value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PublicContextBinding {
    /// The context field identifier.
    pub field: ir::ContextFieldId,
    /// The portable serialized value.
    pub value: PortableValue,
}

/// The public statement committed by a proof: program ID, context, and event digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PublicStatement {
    /// The program that was executed.
    pub program_id: ir::ProgramId,
    /// Committed public context values.
    pub public_context: Vec<PublicContextBinding>,
    /// Digest over all emitted events.
    pub event_digest: Digest,
}

/// Transcript-bound semantic proof statement shared across proving and verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProofStatement {
    /// Canonical proof statement schema version.
    pub schema_version: u32,
    /// Compiler-sealed program binding.
    pub binding: ProgramBinding,
    /// Semantic public statement visible to callers.
    pub public: PublicStatement,
    /// Canonical digest of the applied transaction batch.
    pub applied_tx_digest: Digest,
    /// Transcript-bound root of the sealed static relation table set.
    pub static_table_root: Digest,
    /// Root before batch execution.
    pub old_state_root: Digest,
    /// Root after batch execution.
    pub new_state_root: Digest,
}

impl ProofStatement {
    /// Construct one statement using the current contract schema version.
    #[must_use]
    pub fn new(
        binding: ProgramBinding,
        public: PublicStatement,
        applied_tx_digest: Digest,
        static_table_root: Digest,
        old_state_root: Digest,
        new_state_root: Digest,
    ) -> Self {
        Self {
            schema_version: STATEMENT_SCHEMA_VERSION,
            binding,
            public,
            applied_tx_digest,
            static_table_root,
            old_state_root,
            new_state_root,
        }
    }

    /// Serialize the statement canonically for transcript binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProofContractError> {
        validate_statement_schema_version(self.schema_version)?;

        let canonical = CanonicalProofStatement {
            schema_version: self.schema_version,
            program_hash: *self.binding.program_hash(),
            metadata_hash: *self.binding.metadata_hash(),
            program_id: self.public.program_id,
            public_context: self.public.public_context.clone(),
            event_digest: self.public.event_digest,
            applied_tx_digest: self.applied_tx_digest,
            static_table_root: self.static_table_root,
            old_state_root: self.old_state_root,
            new_state_root: self.new_state_root,
        };

        let mut bytes = PROOF_STATEMENT_DOMAIN.to_vec();
        bytes.extend(borsh::to_vec(&canonical).map_err(|error| {
            ProofContractError::StatementEncode {
                detail: error.to_string(),
            }
        })?);
        Ok(bytes)
    }

    /// Canonical transcript-bound digest.
    pub fn statement_hash_bytes(&self) -> Result<[u8; 32], ProofContractError> {
        Ok(sha2::Sha256::digest(self.canonical_bytes()?).into())
    }
}

/// Canonical proof system identifier used by `proof.bin`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProofSystemId(u16);

impl ProofSystemId {
    /// Native Tabula STARK multi-proof system.
    pub const TABULA_STARK: Self = Self(1);

    /// Raw numeric identifier.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Human-readable stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            1 => "tabula_stark",
            _ => "unknown",
        }
    }

    fn validate(self) -> Result<Self, ProofContractError> {
        match self.0 {
            1 => Ok(self),
            got => Err(ProofContractError::UnknownProofSystemId { got }),
        }
    }
}

/// Canonical proof byte encoding identifier used by `proof.bin`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProofEncodingId(u16);

impl ProofEncodingId {
    /// Machine-owned binary codec version 2.
    pub const TABULA_MACHINE_BINARY_V2: Self = Self(2);

    /// Raw numeric identifier.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Human-readable stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            2 => "tabula_machine_binary_v2",
            _ => "unknown",
        }
    }

    fn validate(self) -> Result<Self, ProofContractError> {
        match self.0 {
            2 => Ok(self),
            got => Err(ProofContractError::UnknownProofEncodingId { got }),
        }
    }
}

/// Canonical `proof.bin` payload shared across SDK and CLI surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProofEnvelopeV2 {
    /// Contract-owned proof statement.
    pub statement: ProofStatement,
    /// Proof system identifier.
    pub proof_system: ProofSystemId,
    /// Proof payload encoding identifier.
    pub proof_encoding: ProofEncodingId,
    /// Opaque proof bytes owned by the machine/backend layer.
    pub proof_bytes: Vec<u8>,
}

impl ProofEnvelopeV2 {
    /// Create one proof envelope for the current contract version.
    #[must_use]
    pub fn new(
        statement: ProofStatement,
        proof_system: ProofSystemId,
        proof_encoding: ProofEncodingId,
        proof_bytes: Vec<u8>,
    ) -> Self {
        Self {
            statement,
            proof_system,
            proof_encoding,
            proof_bytes,
        }
    }

    fn validate(&self) -> Result<(), ProofContractError> {
        validate_statement_schema_version(self.statement.schema_version)?;
        self.proof_system.validate()?;
        self.proof_encoding.validate()?;
        Ok(())
    }
}

/// Encode one canonical proof envelope as `proof.bin`.
pub fn encode_proof_envelope(envelope: &ProofEnvelopeV2) -> Result<Vec<u8>, ProofContractError> {
    envelope.validate()?;
    let mut bytes = Vec::with_capacity(PROOF_ENVELOPE_MAGIC.len() + 4 + 512);
    bytes.extend_from_slice(PROOF_ENVELOPE_MAGIC);
    bytes.extend_from_slice(&PROOF_ENVELOPE_VERSION.to_be_bytes());
    bytes.extend(
        borsh::to_vec(envelope).map_err(|error| ProofContractError::EnvelopeEncode {
            detail: error.to_string(),
        })?,
    );
    Ok(bytes)
}

/// Decode one canonical proof envelope from `proof.bin`.
pub fn decode_proof_envelope(bytes: &[u8]) -> Result<ProofEnvelopeV2, ProofContractError> {
    if bytes.len() < PROOF_ENVELOPE_MAGIC.len() + 4 {
        return Err(ProofContractError::EnvelopeDecode {
            detail: "proof envelope is truncated".to_string(),
        });
    }

    let (magic, rest) = bytes.split_at(PROOF_ENVELOPE_MAGIC.len());
    if magic != PROOF_ENVELOPE_MAGIC {
        return Err(ProofContractError::InvalidEnvelopeMagic);
    }

    let (version_bytes, payload) = rest.split_at(4);
    let version = u32::from_be_bytes(
        version_bytes
            .try_into()
            .expect("proof envelope version is exactly 4 bytes"),
    );
    validate_proof_envelope_version(version)?;

    let envelope = ProofEnvelopeV2::try_from_slice(payload).map_err(|error| {
        ProofContractError::EnvelopeDecode {
            detail: error.to_string(),
        }
    })?;
    envelope.validate()?;
    Ok(envelope)
}
