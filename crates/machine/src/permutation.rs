//! Permutation trace infrastructure for cross-chip LogUp verification.
//!
//! Provides:
//! - Fiat-Shamir challenge derivation from chip proof metadata
//! - Extension-field (EF4) fingerprint computation
//! - Per-chip cumulative sum calculation
//!
//! # Soundness status
//!
//! **C2 (fixed)**: Challenges α, β are derived from a Fiat-Shamir transcript
//! seeded with chip proof metadata (trace heights, public values), not hardcoded.
//!
//! **M5 (fixed)**: Fingerprints are computed in EF4 (~124-bit security), not
//! the base field (~31-bit).
//!
//! **C1 (open)**: Cumulative sums are not yet committed via PCS. This requires
//! a custom two-round prover (bypassing p3-uni-stark) to commit permutation
//! trace columns. Deferred to a future phase.

use p3_baby_bear::{BabyBear, default_babybear_poseidon2_16};
use p3_challenger::{CanObserve, CanSample, DuplexChallenger};
use p3_field::{BasedVectorSpace, PrimeCharacteristicRing};

use tabula_stark::air::interaction::{BusId, InteractionDirection};
use tabula_stark::debug::{ChipRecord, RecordedInteraction};

use super::config::EF4;
use super::proof::ChipProofEntry;

// ─── Challenger construction ─────────────────────────────────────────────────

/// Concrete challenger type for Fiat-Shamir challenge derivation.
type TabChallenger = DuplexChallenger<
    BabyBear,
    p3_poseidon2::Poseidon2<
        BabyBear,
        p3_baby_bear::Poseidon2ExternalLayerBabyBear<16>,
        p3_baby_bear::Poseidon2InternalLayerBabyBear<16>,
        16,
        7,
    >,
    16,
    8,
>;

/// Create a fresh Fiat-Shamir challenger using the default Poseidon2 permutation.
fn new_challenger() -> TabChallenger {
    DuplexChallenger::new(default_babybear_poseidon2_16())
}

// ─── Challenge derivation ────────────────────────────────────────────────────

/// Derive LogUp challenges (α, β) from chip proof metadata via Fiat-Shamir.
///
/// The challenger observes:
/// 1. Each chip's trace height (as a field element)
/// 2. Each chip's public values
///
/// This binds the challenges to the specific proof instance.
///
/// # Note
///
/// Ideally we would observe the PCS commitments (Merkle roots) from each
/// per-chip STARK proof. However, p3-uni-stark's `Proof` type does not
/// expose commitments directly. Observing trace heights + public values
/// provides binding to the proof instance. Full PCS-binding will come
/// when the custom two-round prover is implemented.
pub(crate) fn derive_challenges(chip_proofs: &[ChipProofEntry]) -> [EF4; 2] {
    let mut challenger = new_challenger();

    // Domain separator for LogUp challenges.
    challenger.observe(BabyBear::from_u64(0xDEAD_BEEF));

    for entry in chip_proofs {
        // Observe chip identity via trace height.
        challenger.observe(BabyBear::from_u64(entry.trace_height as u64));

        // Observe public values (binds to the specific batch).
        for &pv in &entry.public_values {
            challenger.observe(pv);
        }
    }

    let alpha: EF4 = challenger.sample();
    let beta: EF4 = challenger.sample();
    [alpha, beta]
}

// ─── EF4 fingerprint computation ─────────────────────────────────────────────

/// Compute an RLC fingerprint in the extension field EF4.
///
/// `f = α + kind_tag + β · values[0] + β² · values[1] + …`
///
/// Using EF4 (~124-bit) instead of BabyBear (~31-bit) provides
/// negligible collision probability.
pub(crate) fn compute_fingerprint_ef4(
    values: &[BabyBear],
    bus: BusId,
    alpha: EF4,
    beta: EF4,
) -> EF4 {
    let mut result = alpha + EF4::from(BabyBear::from_u64(bus.tag() as u64));
    let mut beta_power = beta;
    for &val in values {
        result += beta_power * EF4::from(val);
        beta_power *= beta;
    }
    result
}

// ─── Per-chip cumulative sums ────────────────────────────────────────────────

/// Compute per-chip LogUp cumulative sums using EF4 fingerprints.
///
/// For each chip record, sums `±m/f` over all interactions where:
/// - `f` is the EF4 fingerprint
/// - `m` is the multiplicity (lifted to EF4)
/// - Send contributes `+m/f`, Receive contributes `-m/f`
///
/// Returns one EF4 cumulative sum per chip record.
pub(crate) fn compute_cumsums_ef4(
    records: &[ChipRecord<BabyBear>],
    challenges: [EF4; 2],
) -> Vec<EF4> {
    let [alpha, beta] = challenges;

    records
        .iter()
        .map(|record| compute_chip_cumsum(&record.interactions, alpha, beta))
        .collect()
}

/// Compute the cumulative sum for a single chip's interactions.
fn compute_chip_cumsum(
    interactions: &[RecordedInteraction<BabyBear>],
    alpha: EF4,
    beta: EF4,
) -> EF4 {
    let mut chip_sum = EF4::ZERO;

    for interaction in interactions {
        if interaction.multiplicity == BabyBear::ZERO {
            continue;
        }

        let fingerprint =
            compute_fingerprint_ef4(&interaction.values, interaction.bus, alpha, beta);

        if fingerprint == EF4::ZERO {
            // Zero fingerprint means the prover has found a collision.
            // In a sound system this is negligible (~2^{-124}).
            continue;
        }

        let mult_ef4 = EF4::from(interaction.multiplicity);
        let contribution = mult_ef4 / fingerprint;

        match interaction.direction {
            InteractionDirection::Send => chip_sum += contribution,
            InteractionDirection::Receive => chip_sum -= contribution,
        }
    }

    chip_sum
}

/// Convert an EF4 element to its 4 BabyBear basis coefficients.
pub(crate) fn ef4_to_babybear_array(ef: EF4) -> [BabyBear; 4] {
    let slice = ef.as_basis_coefficients_slice();
    [slice[0], slice[1], slice[2], slice[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_stark::air::interaction::core_buses;
    use tabula_stark::debug::RecordedInteraction;

    fn bb(x: u64) -> BabyBear {
        BabyBear::from_u64(x)
    }

    #[test]
    fn fingerprint_ef4_deterministic() {
        let alpha: EF4 = EF4::from(bb(100));
        let beta: EF4 = EF4::from(bb(200));
        let values = [bb(1), bb(2), bb(3)];

        let f1 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
        let f2 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
        assert_eq!(f1, f2);
    }

    #[test]
    fn fingerprint_ef4_different_bus() {
        let alpha: EF4 = EF4::from(bb(100));
        let beta: EF4 = EF4::from(bb(200));
        let values = [bb(1), bb(2), bb(3)];

        let f1 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
        let f2 = compute_fingerprint_ef4(&values, core_buses::RANGE_CHECK, alpha, beta);
        assert_ne!(f1, f2);
    }

    #[test]
    fn balanced_cumsums_sum_to_zero() {
        let records = vec![
            ChipRecord {
                name: "sender".to_string(),
                interactions: vec![RecordedInteraction {
                    bus: core_buses::READ_ACCESS,
                    values: vec![bb(42)],
                    multiplicity: bb(1),
                    direction: InteractionDirection::Send,
                }],
            },
            ChipRecord {
                name: "receiver".to_string(),
                interactions: vec![RecordedInteraction {
                    bus: core_buses::READ_ACCESS,
                    values: vec![bb(42)],
                    multiplicity: bb(1),
                    direction: InteractionDirection::Receive,
                }],
            },
        ];

        // Use arbitrary challenges.
        let challenges = [EF4::from(bb(12345)), EF4::from(bb(67890))];
        let cumsums = compute_cumsums_ef4(&records, challenges);

        let total: EF4 = cumsums.iter().copied().sum();
        assert_eq!(total, EF4::ZERO);
    }

    #[test]
    fn imbalanced_cumsums_nonzero() {
        let records = vec![ChipRecord {
            name: "sender_only".to_string(),
            interactions: vec![RecordedInteraction {
                bus: core_buses::READ_ACCESS,
                values: vec![bb(42)],
                multiplicity: bb(1),
                direction: InteractionDirection::Send,
            }],
        }];

        let challenges = [EF4::from(bb(12345)), EF4::from(bb(67890))];
        let cumsums = compute_cumsums_ef4(&records, challenges);

        let total: EF4 = cumsums.iter().copied().sum();
        assert_ne!(total, EF4::ZERO);
    }

    #[test]
    fn derive_challenges_deterministic() {
        let entries = vec![ChipProofEntry {
            chip_id: tabula_stark::chips::core_chips::EXECUTION,
            proof: dummy_proof(),
            cumsum_final: EF4::ZERO,
            trace_height: 4,
            public_values: vec![bb(1), bb(2)],
            preprocessed_vk: None,
        }];

        let c1 = derive_challenges(&entries);
        let c2 = derive_challenges(&entries);
        assert_eq!(c1, c2);
    }

    #[test]
    fn derive_challenges_different_inputs() {
        let entries_a = vec![ChipProofEntry {
            chip_id: tabula_stark::chips::core_chips::EXECUTION,
            proof: dummy_proof(),
            cumsum_final: EF4::ZERO,
            trace_height: 4,
            public_values: vec![bb(1), bb(2)],
            preprocessed_vk: None,
        }];
        let entries_b = vec![ChipProofEntry {
            chip_id: tabula_stark::chips::core_chips::EXECUTION,
            proof: dummy_proof(),
            cumsum_final: EF4::ZERO,
            trace_height: 8, // different height
            public_values: vec![bb(1), bb(2)],
            preprocessed_vk: None,
        }];

        let c1 = derive_challenges(&entries_a);
        let c2 = derive_challenges(&entries_b);
        assert_ne!(c1, c2);
    }

    /// Create a dummy proof for testing challenge derivation.
    /// We only need the metadata fields (trace_height, public_values), not the actual proof.
    fn dummy_proof() -> p3_uni_stark::Proof<crate::config::TabulaStarkConfig> {
        // Build a minimal valid proof by running the STARK pipeline on a trivial chip.
        use p3_air::{Air, BaseAir};
        use p3_matrix::dense::RowMajorMatrix;

        #[derive(Clone, Debug)]
        struct TrivialChip;

        impl BaseAir<BabyBear> for TrivialChip {
            fn width(&self) -> usize {
                1
            }
        }

        impl<AB: p3_air::AirBuilder<F = BabyBear>> Air<AB> for TrivialChip {
            fn eval(&self, _builder: &mut AB) {}
        }

        let config = crate::config::default_config();
        let trace = RowMajorMatrix::new(vec![BabyBear::ZERO; 2], 1);
        p3_uni_stark::prove(&config, &TrivialChip, trace, &[])
    }
}
