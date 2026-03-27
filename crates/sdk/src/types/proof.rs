use tabula_contract::{ProofEnvelopeV2, ProofStatement, ProofSystemId, encode_proof_envelope};
use tabula_machine::TabulaProof;
use tabula_machine::{decode_proof_bytes, encode_proof_bytes};
#[cfg(feature = "prove")]
use tabula_runtime::ProofSummary;

use crate::error::SdkError;

/// In-memory proof bundle produced by the SDK.
pub struct Proof {
    pub(crate) proof: TabulaProof,
    pub(crate) statement: ProofStatement,
    #[cfg(feature = "prove")]
    pub(crate) summary: ProofSummary,
}

impl Proof {
    #[cfg(feature = "prove")]
    pub(crate) fn from_prove_result(result: tabula_runtime::ProveResult) -> Self {
        Self {
            proof: result.proof,
            statement: result.statement,
            summary: result.summary,
        }
    }

    /// Returns the statement tied to this proof.
    pub fn statement(&self) -> &ProofStatement {
        &self.statement
    }

    /// Project this proof into the canonical contract-owned proof envelope.
    pub fn to_envelope(&self) -> Result<ProofEnvelopeV2, SdkError> {
        let proof_bytes =
            encode_proof_bytes(&self.proof).map_err(|error| SdkError::ProofDecode {
                detail: error.to_string(),
            })?;
        Ok(ProofEnvelopeV2::new(
            self.statement.clone(),
            ProofSystemId::TABULA_STARK,
            tabula_contract::ProofEncodingId::TABULA_MACHINE_BINARY_V2,
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
    pub fn from_envelope(envelope: ProofEnvelopeV2) -> Result<Self, SdkError> {
        let statement = envelope.statement;
        let expected_digest =
            statement
                .statement_hash_bytes()
                .map_err(|error| SdkError::ProofDecode {
                    detail: error.to_string(),
                })?;
        let proof =
            decode_proof_bytes(&envelope.proof_bytes).map_err(|error| SdkError::ProofDecode {
                detail: error.to_string(),
            })?;
        if proof.statement_digest != expected_digest {
            return Err(SdkError::ProofDecode {
                detail: "proof envelope statement digest does not match the embedded proof"
                    .to_string(),
            });
        }
        Ok(Self {
            #[cfg(feature = "prove")]
            summary: ProofSummary::from_proof(&proof),
            proof,
            statement,
        })
    }

    /// Decode one SDK proof from canonical `proof.bin`.
    pub fn decode_binary(bytes: &[u8]) -> Result<Self, SdkError> {
        let envelope = tabula_contract::decode_proof_envelope(bytes).map_err(|error| {
            SdkError::ProofDecode {
                detail: error.to_string(),
            }
        })?;
        Self::from_envelope(envelope)
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
        debug.field("statement", &self.statement);
        #[cfg(feature = "prove")]
        debug.field("summary", &self.summary);
        debug.finish_non_exhaustive()
    }
}
