//! Multi-chip STARK verifier.
//!
//! Uses `p3-uni-stark::verify()` per chip for main constraint verification,
//! then checks cross-chip LogUp balance via cumulative sum equality.
//!
//! Generic over `CS: ChipSet` — callers specify their chip set at the call site:
//! ```ignore
//! stark::verify::<TabulaAir>(&default_config(), &proof)?;
//! ```

use std::collections::BTreeSet;

use p3_field::PrimeCharacteristicRing;
use p3_uni_stark;

use crate::air::chip_instance::ChipInstance;
use crate::air::chip_set::ChipSet;

use super::config::{EF4, TabulaStarkConfig, default_config};
use super::permutation;
use super::proof::{TabulaProof, VerificationError};
use super::prover::StarkAir;

/// Verify a Tabula STARK proof using the default configuration.
pub fn verify<CS: StarkAir>(proof: &TabulaProof) -> Result<(), VerificationError> {
    verify_with_config::<CS>(&default_config(), proof)
}

/// Verify a Tabula STARK proof with an explicit STARK configuration.
///
/// Steps:
/// 1. Validate the proof contains exactly the expected chip set.
/// 2. Verify Fiat-Shamir challenges match the transcript.
/// 3. For each chip proof, run `p3_uni_stark::verify()`.
/// 4. Sum all chips' final cumulative sums and check equality to zero.
pub fn verify_with_config<CS: StarkAir>(
    config: &TabulaStarkConfig,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    // ── Phase 0: Validate chip manifest ──────────────────────────────────────
    validate_chip_manifest::<CS>(proof)?;

    // ── Phase 1: Per-chip STARK verification ────────────────────────────────
    for entry in &proof.chip_proofs {
        let air = CS::from_name(entry.chip_name).ok_or_else(|| {
            VerificationError::InvalidChipManifest {
                detail: format!("unknown chip name: {}", entry.chip_name),
            }
        })?;

        // ChipInstance without preprocessed data — the verifier receives the
        // preprocessed verifier key from the proof, not the actual trace.
        let instance = ChipInstance::new(air);

        let result = p3_uni_stark::verify_with_preprocessed(
            config,
            &instance,
            &entry.proof,
            &entry.public_values,
            entry.preprocessed_vk.as_ref(),
        );

        if let Err(e) = result {
            return Err(VerificationError::ChipVerificationFailed {
                chip_name: entry.chip_name,
                detail: format!("{e:?}"),
            });
        }
    }

    // ── Phase 2: Verify Fiat-Shamir challenges ───────────────────────────────
    let expected_challenges = permutation::derive_challenges(&proof.chip_proofs);
    if proof.logup_challenges != expected_challenges {
        return Err(VerificationError::ChallengesMismatch {
            expected: expected_challenges,
            got: proof.logup_challenges,
        });
    }

    // ── Phase 3: Cross-chip LogUp balance check ─────────────────────────────
    let mut cumsum_total = EF4::ZERO;
    for entry in &proof.chip_proofs {
        cumsum_total += entry.cumsum_final;
    }

    if cumsum_total != EF4::ZERO {
        return Err(VerificationError::LogUpImbalance {
            total: permutation::ef4_to_babybear_array(cumsum_total),
        });
    }

    Ok(())
}

/// Check that the proof contains exactly the expected set of chips (from `CS::chip_names()`).
fn validate_chip_manifest<CS: ChipSet>(proof: &TabulaProof) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<&str> = proof.chip_proofs.iter().map(|e| e.chip_name).collect();
    let expected: BTreeSet<&str> = CS::chip_names().into_iter().collect();

    if proof_chips.len() != proof.chip_proofs.len() {
        return Err(VerificationError::InvalidChipManifest {
            detail: "duplicate chip names in proof".to_string(),
        });
    }

    let missing: Vec<&str> = expected.difference(&proof_chips).copied().collect();
    let extra: Vec<&str> = proof_chips.difference(&expected).copied().collect();

    if !missing.is_empty() || !extra.is_empty() {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!("missing: {missing:?}, unexpected: {extra:?}"),
        });
    }

    Ok(())
}
