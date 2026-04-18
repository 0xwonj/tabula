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
    ///
    /// Returning a tuple is intentional: the prover has the decoded proof in
    /// hand after proving, and the runtime needs both the wire envelope (for
    /// persistence and transport) and the decoded form (to introspect chip
    /// openings during statement-level verification). The tuple lets callers
    /// skip a decode round-trip without the verifier API having to widen.
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
    /// success.
    ///
    /// The caller supplies the `binding_digest` they expect the proof to be
    /// bound to. The backend re-checks that the digest encoded in the
    /// decoded proof matches this value before running STARK verification,
    /// providing defense-in-depth against envelope-byte tampering that would
    /// otherwise only surface as a transcript-level failure.
    pub fn verify_envelope(
        &self,
        envelope: &ProofEnvelope,
        binding_digest: [u8; 32],
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
        if proof.binding_digest != binding_digest {
            return Err(VerificationError::BindingDigestMismatch {
                expected: binding_digest,
                actual: proof.binding_digest,
            });
        }
        self.verify_proof(&proof)?;
        Ok(proof)
    }

    /// Verify an already-decoded machine proof against this backend's
    /// configured setup.
    ///
    /// Callers that still hold the decoded `TabulaProof` (for example, after
    /// [`BackendProver::prove_envelope`] or while running statement-level
    /// checks against chip openings) can skip re-decoding the envelope bytes.
    ///
    /// **Binding-digest responsibility.** This entry point does *not* compare
    /// `proof.binding_digest` against any caller-supplied expected value. A
    /// valid result here only asserts "this proof was internally consistent
    /// against its own transcript" — not "this proof is bound to the
    /// statement you meant to verify". Callers must either
    /// (a) assert `proof.binding_digest` matches their expected digest
    /// upstream, as the runtime verifier does, or
    /// (b) call [`BackendVerifier::verify_envelope`] instead, which performs
    /// that check before running the STARK verifier.
    pub fn verify_proof(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        self.machine.verify(proof)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_contract::{ProofEncodingId, ProofEnvelope, ProofSystemId};

    use super::{BackendVerifier, VerificationError};
    use crate::config::PcsCommitment;
    use crate::machine::TabulaMachine;
    use crate::proof::codec::encode_proof_bytes;
    use crate::proof::model::{ProofTier, SubProofEnvelope, TabulaProof};

    fn empty_commitment() -> PcsCommitment {
        PcsCommitment::new(vec![[KoalaBear::ZERO; 8]])
    }

    fn empty_opening_proof() -> crate::config::PcsOpeningProof {
        crate::config::PcsOpeningProof {
            commit_phase_commits: vec![],
            commit_pow_witnesses: vec![],
            query_proofs: vec![],
            final_poly: vec![],
            query_pow_witness: KoalaBear::ZERO,
        }
    }

    fn empty_subproof(tier: ProofTier) -> SubProofEnvelope {
        SubProofEnvelope {
            tier,
            preprocessed_commitment: None,
            main_commitment: empty_commitment(),
            perm_commitment: None,
            quotient_commitment: empty_commitment(),
            opening_proof: empty_opening_proof(),
            chip_openings: vec![],
            exported_cumsums: BTreeMap::new(),
        }
    }

    fn envelope_for(proof: &TabulaProof) -> ProofEnvelope {
        let bytes = encode_proof_bytes(proof).expect("encode minimal proof");
        ProofEnvelope::new(
            ProofSystemId::TABULA_STARK,
            ProofEncodingId::TABULA_MACHINE_BINARY_V1,
            bytes,
        )
    }

    #[test]
    fn verify_envelope_rejects_binding_digest_mismatch_before_verifying_proof() {
        let machine = TabulaMachine::new(vec![]).expect("build empty machine");
        let proof = TabulaProof {
            execution: empty_subproof(ProofTier::Execution),
            columns: vec![],
            root: empty_subproof(ProofTier::Root),
            binding_digest: [7u8; 32],
        };
        let envelope = envelope_for(&proof);

        let err = BackendVerifier::new(&machine)
            .verify_envelope(&envelope, [9u8; 32])
            .err()
            .expect("mismatched binding digest must fail");

        match err {
            VerificationError::BindingDigestMismatch { expected, actual } => {
                assert_eq!(expected, [9u8; 32]);
                assert_eq!(actual, [7u8; 32]);
            }
            other => panic!("expected BindingDigestMismatch, got {other:?}"),
        }
    }
}
