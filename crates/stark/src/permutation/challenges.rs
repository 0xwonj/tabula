//! Fiat-Shamir challenge derivation for LogUp.
//!
//! In the batched prover, challenges are derived from the PCS main commitment
//! via the shared Fiat-Shamir transcript. These standalone helpers are retained
//! for unit tests only.

use p3_challenger::{CanObserve, CanSample, DuplexChallenger};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::{KoalaBear, default_koalabear_poseidon2_16};

use crate::EF4;

/// Per-chip metadata for challenge derivation (test only).
pub(crate) struct ChipTraceInfo {
    pub trace_height: usize,
    pub public_values: Vec<KoalaBear>,
}

/// Derive LogUp challenges (α, β) from main trace metadata via Fiat-Shamir (test only).
pub(crate) fn derive_challenges_from_main(chip_infos: &[ChipTraceInfo]) -> [EF4; 2] {
    let mut challenger: DuplexChallenger<_, _, 16, 8> =
        DuplexChallenger::new(default_koalabear_poseidon2_16());

    // Domain separator for LogUp challenges.
    challenger.observe(KoalaBear::from_u64(0xDEAD_BEEF));

    for info in chip_infos {
        challenger.observe(KoalaBear::from_u64(info.trace_height as u64));
        for &pv in &info.public_values {
            challenger.observe(pv);
        }
    }

    let alpha: EF4 = challenger.sample();
    let beta: EF4 = challenger.sample();
    [alpha, beta]
}
