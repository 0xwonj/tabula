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

use tabula_stark::air::chip_instance::ChipInstance;
use tabula_stark::air::chip_set::ChipSet;
use tabula_stark::chips::core_chips;

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
        let air =
            CS::from_id(entry.chip_id).ok_or_else(|| VerificationError::InvalidChipManifest {
                detail: format!("unknown chip id: {}", entry.chip_id),
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
                chip_id: entry.chip_id,
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

    // ── Phase 2b: Verify public statement matches SmtTablePath public values ─
    let expected_pvs = proof.statement.to_field_elements();
    for entry in &proof.chip_proofs {
        if entry.chip_id == core_chips::SMT_TABLE_PATH && entry.public_values != expected_pvs {
            return Err(VerificationError::InvalidChipManifest {
                detail: "SmtTablePath public values do not match proof statement".to_string(),
            });
        }
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

/// Check that the proof contains exactly the expected set of chips (from `CS::chip_ids()`).
fn validate_chip_manifest<CS: ChipSet>(proof: &TabulaProof) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<_> = proof.chip_proofs.iter().map(|e| e.chip_id).collect();
    let expected: BTreeSet<_> = CS::chip_ids().into_iter().collect();

    if proof_chips.len() != proof.chip_proofs.len() {
        return Err(VerificationError::InvalidChipManifest {
            detail: "duplicate chip IDs in proof".to_string(),
        });
    }

    let missing: Vec<_> = expected.difference(&proof_chips).copied().collect();
    let extra: Vec<_> = proof_chips.difference(&expected).copied().collect();

    if !missing.is_empty() || !extra.is_empty() {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!("missing: {missing:?}, unexpected: {extra:?}"),
        });
    }

    Ok(())
}
