use tabula_contract::{ProofEnvelope, PublicStatement, encode_proof_envelope};
use tabula_machine::TabulaProof;
use tabula_machine::decode_proof_bytes;
#[cfg(feature = "prove")]
use tabula_runtime::ProofSummary;

use crate::error::SdkError;

/// In-memory proof bundle produced by the SDK.
pub struct Proof {
    pub(crate) proof: TabulaProof,
    pub(crate) envelope: ProofEnvelope,
    pub(crate) public_statement: Option<PublicStatement>,
    #[cfg(feature = "prove")]
    pub(crate) summary: ProofSummary,
}

impl Proof {
    #[cfg(feature = "prove")]
    pub(crate) fn from_prove_result(result: tabula_runtime::ProveResult) -> Self {
        Self {
            proof: result.proof,
            envelope: result.envelope,
            public_statement: Some(result.public_statement),
            summary: result.summary,
        }
    }

    /// Returns the artifact-bound public statement paired with this proof, if known.
    ///
    /// The statement is present when the SDK produced the proof locally. Proofs
    /// reconstructed from an envelope alone do not carry the statement — callers
    /// must thread it separately.
    pub const fn public_statement(&self) -> Option<&PublicStatement> {
        self.public_statement.as_ref()
    }

    /// Returns the transcript-bound artifact binding digest for this proof.
    pub const fn binding_digest(&self) -> &[u8; 32] {
        &self.proof.binding_digest
    }

    /// Project this proof into the canonical contract-owned proof envelope.
    pub fn to_envelope(&self) -> ProofEnvelope {
        self.envelope.clone()
    }

    /// Encode this proof as canonical `proof.bin`.
    pub fn encode_binary(&self) -> Result<Vec<u8>, SdkError> {
        encode_proof_envelope(&self.envelope).map_err(|error| SdkError::ProofDecode {
            detail: error.to_string(),
        })
    }

    /// Reconstruct one SDK proof from a decoded contract envelope.
    ///
    /// The returned proof has no associated public statement; verification
    /// requires the caller to supply it out of band.
    pub fn from_envelope(envelope: ProofEnvelope) -> Result<Self, SdkError> {
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
            envelope,
            public_statement: None,
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
        debug.field("public_statement", &self.public_statement);
        debug.field("binding_digest", &self.proof.binding_digest);
        #[cfg(feature = "prove")]
        debug.field("summary", &self.summary);
        debug.finish_non_exhaustive()
    }
}
