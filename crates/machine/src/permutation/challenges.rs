//! Fiat-Shamir challenge derivation for LogUp.

use p3_baby_bear::{BabyBear, default_babybear_poseidon2_16};
use p3_challenger::{CanObserve, CanSample, DuplexChallenger};
use p3_field::PrimeCharacteristicRing;

use crate::config::{Challenger, EF4};

/// Create a fresh Fiat-Shamir challenger using the default Poseidon2 permutation.
fn new_challenger() -> Challenger {
    DuplexChallenger::new(default_babybear_poseidon2_16())
}

/// Per-chip metadata for challenge derivation (before proofs exist).
pub(crate) struct ChipTraceInfo {
    pub trace_height: usize,
    pub public_values: Vec<BabyBear>,
}

/// Derive LogUp challenges (α, β) from main trace metadata via Fiat-Shamir.
///
/// Called before permutation trace generation. Binds challenges to:
/// 1. Each chip's trace height
/// 2. Each chip's public values
pub(crate) fn derive_challenges_from_main(chip_infos: &[ChipTraceInfo]) -> [EF4; 2] {
    let mut challenger = new_challenger();

    // Domain separator for LogUp challenges.
    challenger.observe(BabyBear::from_u64(0xDEAD_BEEF));

    for info in chip_infos {
        challenger.observe(BabyBear::from_u64(info.trace_height as u64));
        for &pv in &info.public_values {
            challenger.observe(pv);
        }
    }

    let alpha: EF4 = challenger.sample();
    let beta: EF4 = challenger.sample();
    [alpha, beta]
}

/// Derive LogUp challenges from proof entries (used by verifier).
pub(crate) fn derive_challenges(chip_proofs: &[crate::proof::ChipProofEntry]) -> [EF4; 2] {
    let infos: Vec<ChipTraceInfo> = chip_proofs
        .iter()
        .map(|e| ChipTraceInfo {
            trace_height: e.trace_height,
            public_values: e.public_values.clone(),
        })
        .collect();
    derive_challenges_from_main(&infos)
}
