//! Multi-chip STARK prover.
//!
//! Uses `p3-uni-stark::prove()` per chip for main constraint proofs,
//! then records LogUp interactions and computes cross-chip cumulative sums.

use p3_air::{Air, BaseAir, BaseAirWithPublicValues};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{self, setup_preprocessed};

use crate::air::builder::InteractionAirBuilder;
use crate::air::chips::{ChipMeta, TabulaAir};
use crate::air::debug::{check_logup_balance, evaluate_chip_with_preprocessed_and_public_values};
use crate::trace_builder::AllTraceBundle;

use super::config::{EF4, TabulaStarkConfig, default_config};
use super::proof::{ChipProofEntry, TabulaProof};

/// Standard value width (U64/I64 = 3 BabyBear limbs).
const W: usize = 3;

/// Wrapper around [`TabulaAir`] that carries optional preprocessed trace data.
///
/// p3-uni-stark's internal debug checker calls `BaseAir::preprocessed_trace()` to get
/// preprocessed columns. Our AIR chips are unit structs that can't carry this data,
/// so we wrap them here for the prover.
struct ProverAir {
    inner: TabulaAir,
    preprocessed: Option<RowMajorMatrix<BabyBear>>,
}

impl BaseAir<BabyBear> for ProverAir {
    fn width(&self) -> usize {
        <TabulaAir as BaseAir<BabyBear>>::width(&self.inner)
    }

    fn preprocessed_trace(&self) -> Option<RowMajorMatrix<BabyBear>> {
        self.preprocessed.clone()
    }
}

impl BaseAirWithPublicValues<BabyBear> for ProverAir {
    fn num_public_values(&self) -> usize {
        use crate::air::chips::smt_path::air::SMT_TABLE_PATH_NUM_PUBLIC_VALUES;
        match &self.inner {
            TabulaAir::SmtTablePath(_) => SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
            _ => 0,
        }
    }
}

impl<AB> Air<AB> for ProverAir
where
    AB: InteractionAirBuilder<F = BabyBear> + p3_air::AirBuilderWithPublicValues,
{
    fn eval(&self, builder: &mut AB) {
        self.inner.eval(builder)
    }
}

/// A chip descriptor: AIR + main trace + optional preprocessed + public values.
struct ChipDescriptor {
    air: ProverAir,
    main_trace: RowMajorMatrix<BabyBear>,
    public_values: Vec<BabyBear>,
}

/// Build chip descriptors from an `AllTraceBundle<3>`.
fn build_chip_descriptors(
    bundle: &AllTraceBundle<W>,
    smt_table_path_public_values: &[BabyBear],
) -> Vec<ChipDescriptor> {
    use crate::air::chips::column_meta::ColumnMetaChip;
    use crate::air::chips::execution::ExecutionChip;
    use crate::air::chips::inter_tx_order::InterTxOrderChip;
    use crate::air::chips::poseidon::PoseidonChip;
    use crate::air::chips::range_check::RangeCheckChip;
    use crate::air::chips::smt_path::{SmtColPathChip, SmtTablePathChip};
    use crate::air::chips::state_column::StateColumnChip;
    use crate::air::chips::static_table::StaticTableChip;

    vec![
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::ExecutionStandard(ExecutionChip::<W>),
                preprocessed: None,
            },
            main_trace: bundle.execution_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::InterTxOrderStandard(InterTxOrderChip::<W>),
                preprocessed: None,
            },
            main_trace: bundle.memory.inter_tx_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::StateColumnStandard(StateColumnChip::<W>),
                preprocessed: None,
            },
            main_trace: bundle.memory.state_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::ColumnMeta(ColumnMetaChip),
                preprocessed: None,
            },
            main_trace: bundle.memory.column_meta_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::Poseidon(PoseidonChip),
                preprocessed: Some(bundle.poseidon_preprocessed_trace.clone()),
            },
            main_trace: bundle.poseidon_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::RangeCheck(RangeCheckChip),
                preprocessed: None,
            },
            main_trace: bundle.range_check_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::StaticTableStandard(StaticTableChip::<W>),
                preprocessed: None,
            },
            main_trace: bundle.static_table_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::SmtColPath(SmtColPathChip),
                preprocessed: None,
            },
            main_trace: bundle.smt_col_path_trace.clone(),
            public_values: vec![],
        },
        ChipDescriptor {
            air: ProverAir {
                inner: TabulaAir::SmtTablePath(SmtTablePathChip),
                preprocessed: None,
            },
            main_trace: bundle.smt_table_path_trace.clone(),
            public_values: smt_table_path_public_values.to_vec(),
        },
    ]
}

/// Generate a Tabula STARK proof from a fully assembled trace bundle.
///
/// Steps:
/// 1. Build chip descriptors from the trace bundle.
/// 2. For each chip, run `p3_uni_stark::prove()` to get a per-chip STARK proof.
/// 3. Record LogUp interactions and compute cross-chip cumulative sums.
/// 4. Assemble into a [`TabulaProof`].
///
/// # Panics
///
/// Panics if any chip's trace height is not a power of two (p3 requirement).
pub fn prove(bundle: &AllTraceBundle<W>, smt_table_path_public_values: &[BabyBear]) -> TabulaProof {
    prove_with_config(&default_config(), bundle, smt_table_path_public_values)
}

/// Like [`prove`] but with an explicit STARK configuration.
pub fn prove_with_config(
    config: &TabulaStarkConfig,
    bundle: &AllTraceBundle<W>,
    smt_table_path_public_values: &[BabyBear],
) -> TabulaProof {
    let descriptors = build_chip_descriptors(bundle, smt_table_path_public_values);
    let mut chip_proofs = Vec::with_capacity(descriptors.len());

    // ── Phase 1: Per-chip STARK proofs ──────────────────────────────────────
    for desc in &descriptors {
        let chip_name = desc.air.inner.chip_name();
        let height = desc.main_trace.height();

        // Skip empty chips (height 0 — no rows to prove).
        if height == 0 {
            continue;
        }

        // p3-uni-stark requires power-of-two trace heights.
        assert!(
            height.is_power_of_two(),
            "chip '{chip_name}' trace height {height} is not a power of two"
        );

        // Use setup_preprocessed to commit preprocessed data (if any) for FRI.
        // ProverAir::preprocessed_trace() returns the data for p3's debug checker.
        let degree_bits = height.trailing_zeros() as usize;
        let pp_setup = setup_preprocessed(config, &desc.air, degree_bits);
        let (pp_prover, pp_vk) = match pp_setup {
            Some((prover, vk)) => (Some(prover), Some(vk)),
            None => (None, None),
        };
        let proof = p3_uni_stark::prove_with_preprocessed(
            config,
            &desc.air,
            desc.main_trace.clone(),
            &desc.public_values,
            pp_prover.as_ref(),
        );

        chip_proofs.push(ChipProofEntry {
            chip_name,
            proof,
            cumsum_final: EF4::ZERO, // Populated in Phase 2
            trace_height: height,
            public_values: desc.public_values.clone(),
            preprocessed_vk: pp_vk,
        });
    }

    // ── Phase 2: LogUp cross-chip balance check ─────────────────────────────
    // Record interactions from all chips and verify balance.
    let records = compute_chip_records(&descriptors);

    // Verify LogUp balance (panics on imbalance).
    if let Err(e) = check_logup_balance(&records) {
        panic!("LogUp balance check failed during proving: {e}");
    }

    // Compute per-chip cumulative sums from recorded interactions.
    let cumsums = compute_per_chip_cumsums(&records);
    assert_eq!(
        chip_proofs.len(),
        cumsums.len(),
        "chip_proofs and cumsums must have the same length"
    );
    let mut cumsum_total = EF4::ZERO;
    for (entry, cumsum) in chip_proofs.iter_mut().zip(cumsums.iter()) {
        entry.cumsum_final = *cumsum;
        cumsum_total += *cumsum;
    }

    let cumsum_bytes = ef4_to_babybear_array(cumsum_total);

    TabulaProof {
        chip_proofs,
        cumsum_total: cumsum_bytes,
    }
}

/// Record interactions from all chips for LogUp balance checking.
fn compute_chip_records(
    descriptors: &[ChipDescriptor],
) -> Vec<crate::air::debug::ChipRecord<BabyBear>> {
    let mut records = Vec::new();
    for desc in descriptors {
        if desc.main_trace.height() == 0 {
            continue;
        }
        let record = evaluate_chip_with_preprocessed_and_public_values(
            desc.air.inner.chip_name(),
            &desc.air.inner,
            &desc.main_trace,
            desc.air.preprocessed.as_ref(),
            &desc.public_values,
        )
        .expect("constraint check should pass during proving");
        records.push(record);
    }
    records
}

/// Compute per-chip LogUp cumulative sums from recorded interactions.
///
/// Uses deterministic challenges matching [`crate::air::debug::check_logup_balance`].
fn compute_per_chip_cumsums(records: &[crate::air::debug::ChipRecord<BabyBear>]) -> Vec<EF4> {
    use crate::air::debug::compute_fingerprint;
    use crate::air::interaction::InteractionDirection;

    // Same deterministic challenges as debug::check_logup_balance.
    let alpha = BabyBear::from_u64(0x1234_5678_9ABC_DEF0);
    let beta = BabyBear::from_u64(0xFEDC_BA98_7654_3210);

    records
        .iter()
        .map(|record| {
            let mut chip_sum = EF4::ZERO;
            for interaction in &record.interactions {
                if interaction.multiplicity == BabyBear::ZERO {
                    continue;
                }
                let fingerprint =
                    compute_fingerprint(&interaction.values, interaction.kind, alpha, beta);
                // Skip zero fingerprints to avoid division by zero.
                // NOTE(soundness): In a production system this should be an error,
                // as zero fingerprints indicate a challenge collision. See mod.rs C2/M5.
                if fingerprint == BabyBear::ZERO {
                    continue;
                }
                // Compute m/f in base field, then lift to EF4.
                let contribution = interaction.multiplicity / fingerprint;
                let contribution_ef4 = EF4::from(contribution);
                match interaction.direction {
                    InteractionDirection::Send => chip_sum += contribution_ef4,
                    InteractionDirection::Receive => chip_sum -= contribution_ef4,
                }
            }
            chip_sum
        })
        .collect()
}

/// Convert an EF4 element to 4 BabyBear base field elements.
fn ef4_to_babybear_array(ef: EF4) -> [BabyBear; 4] {
    use p3_field::BasedVectorSpace;
    let slice = ef.as_basis_coefficients_slice();
    [slice[0], slice[1], slice[2], slice[3]]
}
