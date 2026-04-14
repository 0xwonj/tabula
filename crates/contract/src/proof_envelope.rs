//! Contract-owned transport envelope for `proof.bin`.

use crate::error::ProofContractError;
use crate::versions::{PROOF_ENVELOPE_VERSION, validate_proof_envelope_version};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

const PROOF_ENVELOPE_MAGIC: &[u8] = b"tabula.contract.proof";

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
    /// Machine-owned binary codec version 1.
    pub const TABULA_MACHINE_BINARY_V1: Self = Self(1);

    /// Raw numeric identifier.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Human-readable stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            1 => "tabula_machine_binary_v1",
            _ => "unknown",
        }
    }

    fn validate(self) -> Result<Self, ProofContractError> {
        match self.0 {
            1 => Ok(self),
            got => Err(ProofContractError::UnknownProofEncodingId { got }),
        }
    }
}

/// Canonical `proof.bin` payload shared across SDK and CLI surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProofEnvelope {
    /// Proof system identifier.
    pub proof_system: ProofSystemId,
    /// Proof payload encoding identifier.
    pub proof_encoding: ProofEncodingId,
    /// Opaque proof bytes owned by the machine/backend layer.
    pub proof_bytes: Vec<u8>,
}

impl ProofEnvelope {
    /// Create one proof envelope for the current contract version.
    #[must_use]
    pub fn new(
        proof_system: ProofSystemId,
        proof_encoding: ProofEncodingId,
        proof_bytes: Vec<u8>,
    ) -> Self {
        Self {
            proof_system,
            proof_encoding,
            proof_bytes,
        }
    }

    /// Validate proof-envelope identifiers under the fail-closed contract policy.
    pub fn validate(&self) -> Result<(), ProofContractError> {
        self.proof_system.validate()?;
        self.proof_encoding.validate()?;
        Ok(())
    }
}

/// Encode one canonical proof envelope as `proof.bin`.
pub fn encode_proof_envelope(envelope: &ProofEnvelope) -> Result<Vec<u8>, ProofContractError> {
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
pub fn decode_proof_envelope(bytes: &[u8]) -> Result<ProofEnvelope, ProofContractError> {
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

    let envelope = ProofEnvelope::try_from_slice(payload).map_err(|error| {
        ProofContractError::EnvelopeDecode {
            detail: error.to_string(),
        }
    })?;
    envelope.validate()?;
    Ok(envelope)
}
