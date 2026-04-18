//! Canonical backend primitive surface: envelope-returning prover and verifier.
//!
//! The machine crate is a pure backend primitive. Callers prove and verify
//! through a contract-owned [`ProofEnvelope`] around the machine-encoded proof
//! bytes. The artifact-bound `PublicStatement` is threaded separately by the
//! caller; the machine binds only the 32-byte `binding_digest` into its
//! Fiat-Shamir transcript.

use tabula_contract::{ProofEncodingId, ProofEnvelope, ProofSystemId};

use crate::input::PreparedMachineInput;
use crate::machine::TabulaMachine;
use crate::proof::codec::{decode_proof_bytes, encode_proof_bytes};
use crate::proof::errors::{ProveError, VerificationError};
use crate::proof::model::TabulaProof;

/// Borrowed proving facade that produces a wire-format [`ProofEnvelope`].
#[derive(Debug)]
pub struct BackendProver<'a> {
    machine: &'a TabulaMachine,
}

impl<'a> BackendProver<'a> {
    /// Create a prover wrapping a configured machine.
    #[must_use]
    pub fn new(machine: &'a TabulaMachine) -> Self {
        Self { machine }
    }

    /// Generate a proof and return both the decoded `TabulaProof` and the
    /// canonical envelope wrapping its encoded bytes.
    pub fn prove_envelope(
        &self,
        input: PreparedMachineInput,
    ) -> Result<(TabulaProof, ProofEnvelope), ProveError> {
        let proof = self.machine.prove(input)?;
        let bytes = encode_proof_bytes(&proof)?;
        let envelope = ProofEnvelope::new(
            ProofSystemId::TABULA_STARK,
            ProofEncodingId::TABULA_MACHINE_BINARY_V1,
            bytes,
        );
        Ok((proof, envelope))
    }
}

/// Borrowed verification facade that consumes a wire-format [`ProofEnvelope`].
#[derive(Debug)]
pub struct BackendVerifier<'a> {
    machine: &'a TabulaMachine,
}

impl<'a> BackendVerifier<'a> {
    /// Create a verifier wrapping a configured machine.
    #[must_use]
    pub fn new(machine: &'a TabulaMachine) -> Self {
        Self { machine }
    }

    /// Decode the envelope's proof bytes and verify the proof against the
    /// machine's configured backend setup, returning the decoded proof on
    /// success. The `_binding_digest` parameter is reserved for the caller's
    /// expected digest; it is currently absorbed by the Fiat-Shamir
    /// transcript of the underlying STARK verifier (a mismatch surfaces as a
    /// transcript failure), but is threaded on the envelope API so future
    /// versions can add defense-in-depth at this layer without a signature
    /// break.
    pub fn verify_envelope(
        &self,
        envelope: &ProofEnvelope,
        _binding_digest: [u8; 32],
    ) -> Result<TabulaProof, VerificationError> {
        envelope
            .validate()
            .map_err(|error| VerificationError::UnsupportedProofEnvelope {
                detail: error.to_string(),
            })?;
        if envelope.proof_system != ProofSystemId::TABULA_STARK {
            return Err(VerificationError::BackendMismatch {
                detail: format!(
                    "expected proof system '{}' but envelope declares '{}'",
                    ProofSystemId::TABULA_STARK.name(),
                    envelope.proof_system.name(),
                ),
            });
        }
        if envelope.proof_encoding != ProofEncodingId::TABULA_MACHINE_BINARY_V1 {
            return Err(VerificationError::BackendMismatch {
                detail: format!(
                    "expected proof encoding '{}' but envelope declares '{}'",
                    ProofEncodingId::TABULA_MACHINE_BINARY_V1.name(),
                    envelope.proof_encoding.name(),
                ),
            });
        }
        let proof = decode_proof_bytes(&envelope.proof_bytes)?;
        self.verify_proof(&proof)?;
        Ok(proof)
    }

    /// Verify an already-decoded machine proof against this backend's
    /// configured setup.
    ///
    /// Callers that still hold the decoded `TabulaProof` (for example, after
    /// [`BackendProver::prove_envelope`] or while running statement-level
    /// checks against chip openings) can skip re-decoding the envelope bytes.
    pub fn verify_proof(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        self.machine.verify(proof)
    }
}
