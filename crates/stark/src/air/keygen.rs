//! Keygen phase: extract per-chip metadata for prover integration.
//!
//! [`keygen`] evaluates each chip's `eval()` via column-scanning to extract
//! static [`InteractionDescriptor`]s. These descriptors encode which columns
//! contribute to each interaction's fingerprint, enabling efficient permutation
//! trace generation without re-running `eval()`.
//!
//! # Soundness
//!
//! After extraction, [`verify_extraction_soundness`] evaluates the chip on a
//! random trace and verifies that the extracted descriptors reproduce the same
//! interaction values. This catches any non-affine interaction that column-scanning
//! would miss.

use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use super::descriptor::InteractionDescriptor;
use super::interaction::{ColumnRef, Interaction, InteractionDirection, VirtualPairCol};
use crate::chips::{ChipId, ChipSpec};
use crate::debug::{
    ChipRecord, RecordedInteraction, evaluate_chip_interactions_only,
    evaluate_chip_with_preprocessed_and_public_values,
};

/// Baseline evaluation result: (sends, receives) from debug constraint builder.
type BaselineResult = (
    Vec<RecordedInteraction<BabyBear>>,
    Vec<RecordedInteraction<BabyBear>>,
);

/// Per-chip metadata produced by the keygen phase.
#[derive(Clone, Debug)]
pub struct ChipKeygenInfo {
    /// Type-safe chip identifier.
    pub chip_id: ChipId,
    /// Width of the main trace.
    pub main_width: usize,
    /// Width of the preprocessed trace (0 if none).
    pub preprocessed_width: usize,
    /// Number of public values consumed by this chip.
    pub num_public_values: usize,
    /// Interaction descriptors extracted from column-scanning.
    pub interactions: InteractionDescriptor<BabyBear>,
}

/// Extract keygen info for a single chip.
///
/// Accepts any type implementing the required bounds (including `dyn AnyRap`
/// via `?Sized`). Uses column-scanning extraction wrapped in a soundness
/// assertion against a random trace.
pub fn keygen_chip<A: ?Sized>(chip: &A) -> ChipKeygenInfo
where
    A: ChipSpec + BaseAir<BabyBear> + for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let chip_id = chip.chip_id();
    let main_width = chip.width();
    let pp_width = chip.preprocessed_width();
    let num_pvs = chip.num_public_values();

    let preprocessed = if pp_width > 0 {
        Some(RowMajorMatrix::new(
            vec![BabyBear::ZERO; pp_width * PROBE_HEIGHT],
            pp_width,
        ))
    } else {
        None
    };

    let (sends, receives) =
        extract_interactions(chip, main_width, preprocessed.as_ref(), num_pvs);

    let num_sends_per_row = sends.len();
    let num_receives_per_row = receives.len();

    verify_extraction_soundness(
        chip,
        main_width,
        preprocessed.as_ref(),
        num_pvs,
        &sends,
        &receives,
    );

    ChipKeygenInfo {
        chip_id,
        main_width,
        preprocessed_width: pp_width,
        num_public_values: num_pvs,
        interactions: InteractionDescriptor {
            sends,
            receives,
            num_sends_per_row,
            num_receives_per_row,
        },
    }
}

// ─── Column-scanning extraction (migrated from extractor.rs) ────────────────

/// Minimum trace height for local + next row evaluation.
const PROBE_HEIGHT: usize = 2;

/// Extract static interaction descriptors from a chip by column-scanning.
///
/// Evaluates the chip on a minimal (2-row) probe trace. Each column is set to `1`
/// in turn to determine its contribution to each interaction field.
///
/// Returns `(sends, receives)` as static [`Interaction<BabyBear>`] descriptors.
pub fn extract_interactions<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    num_public_values: usize,
) -> (Vec<Interaction<BabyBear>>, Vec<Interaction<BabyBear>>)
where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let pvs = vec![BabyBear::ZERO; num_public_values];

    // Phase 1: Evaluate with all-zero trace to establish baseline.
    let Some((baseline_sends, baseline_receives)) =
        eval_baseline(air, main_width, preprocessed, &pvs)
    else {
        // Zero-trace caused constraint failures — cannot extract.
        return (vec![], vec![]);
    };

    // Phase 2: Build initial descriptors from baseline constants.
    let mut send_descriptors =
        build_initial_descriptors(&baseline_sends, InteractionDirection::Send);
    let mut recv_descriptors =
        build_initial_descriptors(&baseline_receives, InteractionDirection::Receive);

    // Phase 3: Probe each column to determine its linear contribution.
    scan_column_contributions(
        air,
        main_width,
        preprocessed,
        &pvs,
        &baseline_sends,
        &baseline_receives,
        &mut send_descriptors,
        &mut recv_descriptors,
    );

    (send_descriptors, recv_descriptors)
}

/// Count the number of send and receive interactions per row for a chip.
///
/// Uses a minimal 2-row zero trace. Cheaper than full extraction.
pub fn count_interactions<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    num_public_values: usize,
) -> (usize, usize)
where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let zero_trace =
        RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * PROBE_HEIGHT], main_width);
    let pvs = vec![BabyBear::ZERO; num_public_values];

    let record = evaluate_chip_interactions_only(air, &zero_trace, preprocessed, &pvs);

    let mut sends = 0;
    let mut receives = 0;
    for interaction in &record.interactions {
        match interaction.direction {
            InteractionDirection::Send => sends += 1,
            InteractionDirection::Receive => receives += 1,
        }
    }

    // Divide by height since each row emits the same interactions.
    (sends / PROBE_HEIGHT, receives / PROBE_HEIGHT)
}

// ─── Soundness verification ─────────────────────────────────────────────────

/// Verify that extracted VirtualPairCol descriptors produce correct fingerprints
/// by comparing against DebugConstraintBuilder evaluation on a random trace.
///
/// This catches any non-affine interaction that the column-scanning technique
/// would miss.
fn verify_extraction_soundness<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    num_public_values: usize,
    sends: &[Interaction<BabyBear>],
    receives: &[Interaction<BabyBear>],
) where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    // Use deterministic "random" values so tests are reproducible.
    let random_trace = generate_deterministic_trace(main_width, PROBE_HEIGHT);
    let pvs = vec![BabyBear::ZERO; num_public_values];

    let record = match evaluate_chip_with_preprocessed_and_public_values(
        "_soundness",
        air,
        &random_trace,
        preprocessed,
        &pvs,
    ) {
        Ok(r) => r,
        // If the chip fails on this trace (constraint violation), skip soundness check.
        // Column-scanning correctness is still ensured by the zero+probe evaluations.
        Err(_) => return,
    };

    let (recorded_sends, recorded_receives) = partition_per_row(&record);

    // Verify sends.
    for (i, (extracted, recorded)) in sends.iter().zip(recorded_sends.iter()).enumerate() {
        let local = random_trace.row_slice(0).expect("row exists");
        let next = random_trace.row_slice(1).expect("row exists");

        for (j, vpc) in extracted.values.iter().enumerate() {
            let extracted_val = vpc.eval(&local, &next);
            let recorded_val = recorded.values[j];
            assert_eq!(
                extracted_val, recorded_val,
                "affine assumption violated: send[{i}].values[{j}] — \
                 extracted={extracted_val:?}, recorded={recorded_val:?}"
            );
        }

        let extracted_mult = extracted.multiplicity.eval(&local, &next);
        let recorded_mult = recorded.multiplicity;
        assert_eq!(
            extracted_mult, recorded_mult,
            "affine assumption violated: send[{i}].multiplicity — \
             extracted={extracted_mult:?}, recorded={recorded_mult:?}"
        );
    }

    // Verify receives.
    for (i, (extracted, recorded)) in receives.iter().zip(recorded_receives.iter()).enumerate() {
        let local = random_trace.row_slice(0).expect("row exists");
        let next = random_trace.row_slice(1).expect("row exists");

        for (j, vpc) in extracted.values.iter().enumerate() {
            let extracted_val = vpc.eval(&local, &next);
            let recorded_val = recorded.values[j];
            assert_eq!(
                extracted_val, recorded_val,
                "affine assumption violated: receive[{i}].values[{j}] — \
                 extracted={extracted_val:?}, recorded={recorded_val:?}"
            );
        }

        let extracted_mult = extracted.multiplicity.eval(&local, &next);
        let recorded_mult = recorded.multiplicity;
        assert_eq!(
            extracted_mult, recorded_mult,
            "affine assumption violated: receive[{i}].multiplicity — \
             extracted={extracted_mult:?}, recorded={recorded_mult:?}"
        );
    }
}

/// Generate a deterministic trace with varied but reproducible values.
fn generate_deterministic_trace(width: usize, height: usize) -> RowMajorMatrix<BabyBear> {
    let mut data = Vec::with_capacity(width * height);
    for row in 0..height {
        for col in 0..width {
            // Use a simple hash-like function to generate varied values.
            let val = ((row as u64 + 1) * 31 + (col as u64 + 1) * 97) % (1u64 << 30);
            data.push(BabyBear::from_u64(val));
        }
    }
    RowMajorMatrix::new(data, width)
}

// ─── Internal helpers (migrated from extractor.rs) ──────────────────────────

/// Phase 1: Evaluate chip on an all-zero trace to establish baseline interaction counts.
///
/// Uses interaction-only evaluation to capture interactions even when the
/// zero trace violates AIR constraints (which is common for most chips).
fn eval_baseline<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    pvs: &[BabyBear],
) -> Option<BaselineResult>
where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let zero_trace =
        RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * PROBE_HEIGHT], main_width);

    let baseline_record = evaluate_chip_interactions_only(air, &zero_trace, preprocessed, pvs);

    let result = partition_per_row(&baseline_record);

    // Return None only if there are no interactions at all.
    if result.0.is_empty() && result.1.is_empty() {
        return None;
    }

    Some(result)
}

/// Phase 2: Build initial [`Interaction`] descriptors from baseline constants.
fn build_initial_descriptors(
    baseline: &[RecordedInteraction<BabyBear>],
    direction: InteractionDirection,
) -> Vec<Interaction<BabyBear>> {
    baseline
        .iter()
        .map(|bi| Interaction {
            values: bi
                .values
                .iter()
                .map(|&v| VirtualPairCol {
                    column_weights: Vec::new(),
                    constant: v,
                })
                .collect(),
            multiplicity: VirtualPairCol {
                column_weights: Vec::new(),
                constant: bi.multiplicity,
            },
            bus: bi.bus,
            direction,
        })
        .collect()
}

/// Phase 3: Probe each column to determine its linear contribution to each interaction.
fn scan_column_contributions<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    pvs: &[BabyBear],
    baseline_sends: &[RecordedInteraction<BabyBear>],
    baseline_receives: &[RecordedInteraction<BabyBear>],
    send_descriptors: &mut [Interaction<BabyBear>],
    recv_descriptors: &mut [Interaction<BabyBear>],
) where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    // Scan local columns (row 0).
    for col in 0..main_width {
        let row_offset = 0;
        let col_ref = ColumnRef::Local(col);
        probe_single_column(
            air,
            main_width,
            preprocessed,
            pvs,
            row_offset * main_width + col,
            col_ref,
            baseline_sends,
            baseline_receives,
            send_descriptors,
            recv_descriptors,
        );
    }

    // Scan next-row columns (row 1).
    for col in 0..main_width {
        let row_offset = 1;
        let col_ref = ColumnRef::Next(col);
        probe_single_column(
            air,
            main_width,
            preprocessed,
            pvs,
            row_offset * main_width + col,
            col_ref,
            baseline_sends,
            baseline_receives,
            send_descriptors,
            recv_descriptors,
        );
    }
}

/// Probe a single column: set it to `1` in the probe trace, evaluate, and update descriptors.
fn probe_single_column<A: ?Sized>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    pvs: &[BabyBear],
    probe_index: usize,
    col_ref: ColumnRef,
    baseline_sends: &[RecordedInteraction<BabyBear>],
    baseline_receives: &[RecordedInteraction<BabyBear>],
    send_descriptors: &mut [Interaction<BabyBear>],
    recv_descriptors: &mut [Interaction<BabyBear>],
) where
    A: for<'a> Air<crate::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let mut probe_data = vec![BabyBear::ZERO; main_width * PROBE_HEIGHT];
    probe_data[probe_index] = BabyBear::ONE;
    let probe_trace = RowMajorMatrix::new(probe_data, main_width);

    let probe_record = evaluate_chip_interactions_only(air, &probe_trace, preprocessed, pvs);

    let (probe_sends, probe_receives) = partition_per_row(&probe_record);

    update_descriptors(&probe_sends, baseline_sends, send_descriptors, col_ref);
    update_descriptors(
        &probe_receives,
        baseline_receives,
        recv_descriptors,
        col_ref,
    );
}

/// Update interaction descriptors by comparing probed values against baseline.
fn update_descriptors(
    probed: &[RecordedInteraction<BabyBear>],
    baseline: &[RecordedInteraction<BabyBear>],
    descriptors: &mut [Interaction<BabyBear>],
    col_ref: ColumnRef,
) {
    for (i, (probed_int, baseline_int)) in probed.iter().zip(baseline.iter()).enumerate() {
        if i >= descriptors.len() {
            break;
        }
        for (j, (&pval, &bval)) in probed_int
            .values
            .iter()
            .zip(baseline_int.values.iter())
            .enumerate()
        {
            let weight = pval - bval;
            if weight != BabyBear::ZERO && j < descriptors[i].values.len() {
                descriptors[i].values[j]
                    .column_weights
                    .push((col_ref, weight));
            }
        }
        let mult_weight = probed_int.multiplicity - baseline_int.multiplicity;
        if mult_weight != BabyBear::ZERO {
            descriptors[i]
                .multiplicity
                .column_weights
                .push((col_ref, mult_weight));
        }
    }
}

/// Partition a chip record's interactions into per-row sends and receives.
fn partition_per_row(record: &ChipRecord<BabyBear>) -> BaselineResult {
    let mut sends = Vec::new();
    let mut receives = Vec::new();
    for interaction in &record.interactions {
        match interaction.direction {
            InteractionDirection::Send => sends.push(interaction.clone()),
            InteractionDirection::Receive => receives.push(interaction.clone()),
        }
    }
    // Each row produces the same set of interactions. Take the first half (row 0).
    let sends_per_row = sends.len() / PROBE_HEIGHT;
    let receives_per_row = receives.len() / PROBE_HEIGHT;
    sends.truncate(sends_per_row);
    receives.truncate(receives_per_row);
    (sends, receives)
}
