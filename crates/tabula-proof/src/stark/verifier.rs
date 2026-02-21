//! Multi-chip STARK verifier.
//!
//! Uses `p3-uni-stark::verify()` per chip for main constraint verification,
//! then checks cross-chip LogUp balance via cumulative sum equality.

use std::collections::BTreeSet;

use p3_air::{BaseAir, BaseAirWithPublicValues};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_uni_stark;

use crate::air::chips::TabulaAir;

use super::config::{EF4, TabulaStarkConfig, default_config};
use super::proof::{TabulaProof, VerificationError};

/// The complete set of chip names expected in a valid proof.
const EXPECTED_CHIPS: &[&str] = &[
    "Execution",
    "InterTxOrder",
    "StateColumn",
    "ColumnMeta",
    "Poseidon",
    "RangeCheck",
    "StaticTable",
    "SmtColPath",
    "SmtTablePath",
];

/// Verify a Tabula STARK proof.
///
/// Steps:
/// 1. Validate the proof contains exactly the expected chip set.
/// 2. For each chip proof, run `p3_uni_stark::verify()`.
/// 3. Sum all chips' final cumulative sums and check equality to zero.
pub fn verify(proof: &TabulaProof) -> Result<(), VerificationError> {
    verify_with_config(&default_config(), proof)
}

/// Like [`verify`] but with an explicit STARK configuration.
pub fn verify_with_config(
    config: &TabulaStarkConfig,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    // ── Phase 0: Validate chip manifest ──────────────────────────────────────
    validate_chip_manifest(proof)?;

    // ── Phase 1: Per-chip STARK verification ────────────────────────────────
    for entry in &proof.chip_proofs {
        let air = verifier_air_for_chip(entry.chip_name)?;

        let result = p3_uni_stark::verify_with_preprocessed(
            config,
            &air,
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

    // ── Phase 2: Cross-chip LogUp balance check ─────────────────────────────
    let mut cumsum_total = EF4::ZERO;
    for entry in &proof.chip_proofs {
        cumsum_total += entry.cumsum_final;
    }

    if cumsum_total != EF4::ZERO {
        use p3_field::BasedVectorSpace;
        let slice = cumsum_total.as_basis_coefficients_slice();
        return Err(VerificationError::LogUpImbalance {
            total: [slice[0], slice[1], slice[2], slice[3]],
        });
    }

    Ok(())
}

/// Check that the proof contains exactly the expected set of chips (no missing, no extra, no duplicates).
fn validate_chip_manifest(proof: &TabulaProof) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<&str> = proof.chip_proofs.iter().map(|e| e.chip_name).collect();
    let expected: BTreeSet<&str> = EXPECTED_CHIPS.iter().copied().collect();

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

/// Wrapper AIR for the verifier. The verifier doesn't need preprocessed data
/// (it gets the preprocessed VK from the proof), but p3's verify still calls
/// `BaseAir::width()`.
struct VerifierAir {
    inner: TabulaAir,
}

impl BaseAir<BabyBear> for VerifierAir {
    fn width(&self) -> usize {
        <TabulaAir as BaseAir<BabyBear>>::width(&self.inner)
    }
}

impl BaseAirWithPublicValues<BabyBear> for VerifierAir {
    fn num_public_values(&self) -> usize {
        use crate::air::chips::smt_path::air::SMT_TABLE_PATH_NUM_PUBLIC_VALUES;
        match &self.inner {
            TabulaAir::SmtTablePath(_) => SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
            _ => 0,
        }
    }
}

impl<AB> p3_air::Air<AB> for VerifierAir
where
    AB: crate::air::builder::InteractionAirBuilder<F = BabyBear>
        + p3_air::AirBuilderWithPublicValues,
{
    fn eval(&self, builder: &mut AB) {
        self.inner.eval(builder)
    }
}

/// Reconstruct a verifier AIR from a chip name string.
///
/// Returns an error for unknown chip names instead of panicking.
fn verifier_air_for_chip(name: &str) -> Result<VerifierAir, VerificationError> {
    use crate::air::chips::column_meta::ColumnMetaChip;
    use crate::air::chips::execution::ExecutionChip;
    use crate::air::chips::inter_tx_order::InterTxOrderChip;
    use crate::air::chips::poseidon::PoseidonChip;
    use crate::air::chips::range_check::RangeCheckChip;
    use crate::air::chips::smt_path::{SmtColPathChip, SmtTablePathChip};
    use crate::air::chips::state_column::StateColumnChip;
    use crate::air::chips::static_table::StaticTableChip;

    let inner = match name {
        "Execution" => TabulaAir::ExecutionStandard(ExecutionChip::<3>),
        "InterTxOrder" => TabulaAir::InterTxOrderStandard(InterTxOrderChip::<3>),
        "StateColumn" => TabulaAir::StateColumnStandard(StateColumnChip::<3>),
        "ColumnMeta" => TabulaAir::ColumnMeta(ColumnMetaChip),
        "Poseidon" => TabulaAir::Poseidon(PoseidonChip),
        "RangeCheck" => TabulaAir::RangeCheck(RangeCheckChip),
        "StaticTable" => TabulaAir::StaticTableStandard(StaticTableChip::<3>),
        "SmtColPath" => TabulaAir::SmtColPath(SmtColPathChip),
        "SmtTablePath" => TabulaAir::SmtTablePath(SmtTablePathChip),
        _ => {
            return Err(VerificationError::InvalidChipManifest {
                detail: format!("unknown chip name: {name}"),
            });
        }
    };

    Ok(VerifierAir { inner })
}
