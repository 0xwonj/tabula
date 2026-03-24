//! Machine-level Fiat-Shamir transcript orchestration.
//!
//! This module owns the cross-proof transcript rules shared by the top-level
//! `Prover` and `Verifier`: statement binding, tier commitment ordering, and
//! shared LogUp challenge derivation.

use p3_challenger::{CanObserve, CanSample};
use p3_koala_bear::KoalaBear;
use p3_uni_stark::StarkGenericConfig;

use tabula_stark::air::statement::PublicStatement;

use crate::config::{Challenger, EF4, TabulaStarkConfig};
use crate::proof::instance::MainCommitment;
use crate::proof::model::SubProofEnvelope;

/// Transcript wrapper for machine-level proving and verification orchestration.
///
/// This does not model sub-proof-local transcript steps (for example per-proof
/// quotient commitments and opening challenges). Those stay in the lower-level
/// proof-instance and verifier modules.
pub(crate) struct MachineTranscript {
    challenger: Challenger,
}

impl MachineTranscript {
    /// Start a fresh machine-level transcript from the STARK configuration.
    pub(crate) fn new(config: &TabulaStarkConfig) -> Self {
        Self {
            challenger: config.initialise_challenger(),
        }
    }

    /// Observe the canonical execution statement binding.
    pub(crate) fn observe_statement_binding(
        &mut self,
        statement: &PublicStatement,
        statement_digest: &[u8; 32],
    ) {
        let statement_felts = statement.to_field_elements();
        self.challenger.observe_slice(&statement_felts);

        let digest_felts: Vec<_> = statement_digest
            .iter()
            .map(|byte| KoalaBear::new(u32::from(*byte)))
            .collect();
        self.challenger.observe_slice(&digest_felts);
    }

    /// Observe a proving-side main commitment.
    pub(crate) fn observe_main_commitment(&mut self, commitment: &MainCommitment) {
        if let Some(ref preprocessed) = commitment.preprocessed {
            self.challenger.observe(preprocessed);
        }
        self.challenger.observe(&commitment.main);
    }

    /// Observe a verification-side sub-proof envelope commitment.
    pub(crate) fn observe_envelope_commitment(&mut self, envelope: &SubProofEnvelope) {
        if let Some(ref preprocessed) = envelope.preprocessed_commitment {
            self.challenger.observe(preprocessed);
        }
        self.challenger.observe(&envelope.main_commitment);
    }

    /// Sample the shared LogUp challenges after all global commitments are observed.
    pub(crate) fn sample_logup_challenges(&mut self) -> [EF4; 2] {
        [self.challenger.sample(), self.challenger.sample()]
    }

    /// Fork the current transcript state for an independent sub-proof branch.
    pub(crate) fn fork(&self) -> Challenger {
        self.challenger.clone()
    }
}
