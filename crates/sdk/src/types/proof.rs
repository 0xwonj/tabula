use tabula_contract::{ProofEnvelope, ProofSystemId, encode_proof_envelope};
use tabula_machine::TabulaProof;
use tabula_machine::{decode_proof_bytes, encode_proof_bytes};
#[cfg(feature = "prove")]
use tabula_runtime::ProofSummary;

use crate::error::SdkError;

/// In-memory proof bundle produced by the SDK.
pub struct Proof {
    pub(crate) proof: TabulaProof,
    #[cfg(feature = "prove")]
    pub(crate) summary: ProofSummary,
}

impl Proof {
    #[cfg(feature = "prove")]
    pub(crate) fn from_prove_result(result: tabula_runtime::ProveResult) -> Self {
        Self {
            proof: result.proof,
            summary: result.summary,
        }
    }

    /// Returns the AIR-proved public statement carried by the machine proof.
    pub const fn public_statement(&self) -> &tabula_contract::PublicStatement {
        &self.proof.public_statement
    }

    /// Returns the transcript-bound artifact binding digest for this proof.
    pub const fn binding_digest(&self) -> &[u8; 32] {
        &self.proof.binding_digest
    }

    /// Project this proof into the canonical contract-owned proof envelope.
    pub fn to_envelope(&self) -> Result<ProofEnvelope, SdkError> {
        let proof_bytes =
            encode_proof_bytes(&self.proof).map_err(|error| SdkError::ProofDecode {
                detail: error.to_string(),
            })?;
        Ok(ProofEnvelope::new(
            ProofSystemId::TABULA_STARK,
            tabula_contract::ProofEncodingId::TABULA_MACHINE_BINARY_V1,
            proof_bytes,
        ))
    }

    /// Encode this proof as canonical `proof.bin`.
    pub fn encode_binary(&self) -> Result<Vec<u8>, SdkError> {
        encode_proof_envelope(&self.to_envelope()?).map_err(|error| SdkError::ProofDecode {
            detail: error.to_string(),
        })
    }

    /// Reconstruct one SDK proof from a decoded contract envelope.
    pub fn from_envelope(envelope: &ProofEnvelope) -> Result<Self, SdkError> {
        envelope.validate().map_err(|error| SdkError::ProofDecode {
            detail: error.to_string(),
        })?;
        let proof =
            decode_proof_bytes(&envelope.proof_bytes).map_err(|error| SdkError::ProofDecode {
                detail: error.to_string(),
            })?;
        Ok(Self {
            #[cfg(feature = "prove")]
            summary: ProofSummary::from_proof(&proof),
            proof,
        })
    }

    /// Decode one SDK proof from canonical `proof.bin`.
    pub fn decode_binary(bytes: &[u8]) -> Result<Self, SdkError> {
        let envelope = tabula_contract::decode_proof_envelope(bytes).map_err(|error| {
            SdkError::ProofDecode {
                detail: error.to_string(),
            }
        })?;
        Self::from_envelope(&envelope)
    }

    /// Returns the proof summary when this build enables proving support.
    #[cfg(feature = "prove")]
    pub fn summary(&self) -> &ProofSummary {
        &self.summary
    }
}

impl std::fmt::Debug for Proof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Proof");
        debug.field("public_statement", &self.proof.public_statement);
        debug.field("binding_digest", &self.proof.binding_digest);
        #[cfg(feature = "prove")]
        debug.field("summary", &self.summary);
        debug.finish_non_exhaustive()
    }
}
