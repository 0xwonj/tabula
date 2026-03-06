//! Multi-chip STARK verifier with RAP (Randomized AIR with Preprocessing).
//!
//! For chips with LogUp interactions, uses a two-phase constraint verification
//! that mirrors the prover's approach:
//!
//! 1. **Phase 1** (inner chip): Evaluates the chip's native AIR constraints
//!    against truncated opened values (main trace columns only).
//! 2. **Phase 2** (RAP): Evaluates permutation constraints against the full
//!    combined opened values (main ∥ perm columns).
//!
//! The cumsum is PCS-committed as part of the combined trace, fixing the C1
//! soundness gap.

mod pipeline;
pub(crate) mod rap_folder;

pub(crate) use rap_folder::RapVerifierFolder;

use std::collections::BTreeSet;

use p3_field::PrimeCharacteristicRing;

use crate::chip_ref::ChipRef;
use crate::config::{EF4, TabulaStarkConfig};
use crate::keys::TabulaVerifyingKey;
use crate::permutation;
use crate::proof::{TabulaProof, VerificationError};
use crate::registry::ChipRegistry;

use pipeline::{verify_chip_rap, verify_chip_standard};

// ─── Registry-based verifier ────────────────────────────────────────────────

/// Verify a Tabula STARK proof using [`ChipRegistry`] for AIR dispatch
/// and [`TabulaVerifyingKey`] for cached metadata.
pub fn verify_with_key(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    vk: &TabulaVerifyingKey,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    // Phase 0: Validate chip manifest against the verifying key.
    validate_chip_manifest(vk, proof)?;

    // Phase 1: Per-chip verification.
    for entry in &proof.chip_proofs {
        let air = registry
            .get(entry.chip_id)
            .ok_or_else(|| VerificationError::InvalidChipManifest {
                detail: format!("unknown chip id: {}", entry.chip_id),
            })?;
        let chip_ref = ChipRef::new(air);

        let info =
            vk.get(entry.chip_id)
                .ok_or_else(|| VerificationError::InvalidChipManifest {
                    detail: format!("no verify info for chip {}", entry.chip_id),
                })?;

        if info.interactions_per_row == 0 {
            verify_chip_standard(config, &chip_ref, entry)?;
        } else {
            verify_chip_rap(config, &chip_ref, entry, proof.logup_challenges)?;
        }
    }

    // Phase 2: Verify Fiat-Shamir challenges.
    let expected_challenges = permutation::derive_challenges(&proof.chip_proofs);
    if proof.logup_challenges != expected_challenges {
        return Err(VerificationError::ChallengesMismatch {
            expected: expected_challenges,
            got: proof.logup_challenges,
        });
    }

    // Phase 2b: Verify public values for chips that declare them.
    let expected_pvs = proof.statement.to_field_elements();
    for entry in &proof.chip_proofs {
        let info =
            vk.get(entry.chip_id)
                .ok_or_else(|| VerificationError::InvalidChipManifest {
                    detail: format!("no verify info for chip {}", entry.chip_id),
                })?;
        if info.num_public_values > 0 && entry.public_values != expected_pvs {
            return Err(VerificationError::InvalidChipManifest {
                detail: format!(
                    "chip {} public values do not match proof statement",
                    entry.chip_id
                ),
            });
        }
    }

    // Phase 3: Cross-chip LogUp balance check.
    let mut cumsum_total = EF4::ZERO;
    for entry in &proof.chip_proofs {
        cumsum_total += entry.cumsum_final;
    }

    if cumsum_total != EF4::ZERO {
        return Err(VerificationError::LogUpImbalance {
            total: crate::ef4::ef4_coeffs(cumsum_total),
        });
    }

    Ok(())
}

/// Check that the proof contains exactly the expected set of chips.
fn validate_chip_manifest(
    vk: &TabulaVerifyingKey,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<_> = proof.chip_proofs.iter().map(|e| e.chip_id).collect();
    let expected: BTreeSet<_> = vk.chip_ids().into_iter().collect();

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
