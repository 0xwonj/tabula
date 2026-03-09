//! Fiat-Shamir challenge derivation for LogUp.
//!
//! In the batched prover, challenges are derived from the PCS main commitment
//! via the shared Fiat-Shamir transcript. These standalone helpers are retained
//! for unit tests only.

#[cfg(test)]
use p3_baby_bear::{BabyBear, default_babybear_poseidon2_16};
#[cfg(test)]
use p3_challenger::{CanObserve, CanSample, DuplexChallenger};
#[cfg(test)]
use p3_field::PrimeCharacteristicRing;

#[cfg(test)]
use crate::config::{Challenger, EF4};

/// Per-chip metadata for challenge derivation (test only).
#[cfg(test)]
pub(crate) struct ChipTraceInfo {
    pub trace_height: usize,
    pub public_values: Vec<BabyBear>,
}

/// Derive LogUp challenges (α, β) from main trace metadata via Fiat-Shamir (test only).
#[cfg(test)]
pub(crate) fn derive_challenges_from_main(chip_infos: &[ChipTraceInfo]) -> [EF4; 2] {
    let mut challenger: Challenger = DuplexChallenger::new(default_babybear_poseidon2_16());

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
