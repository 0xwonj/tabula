//! Interaction extraction from AIR chips.
//!
//! [`extract_interactions`] evaluates a chip's `eval()` on a concrete probe trace
//! to extract static [`Interaction<F>`] descriptors. These descriptors encode which
//! columns contribute to each interaction's fingerprint, enabling efficient
//! permutation trace generation without re-running `eval()`.
//!
//! # Approach
//!
//! Uses a column-scanning technique: evaluate the chip once with an all-zero trace
//! to establish baseline interaction constants, then evaluate once per column with
//! that column set to `1` to determine linear dependencies. The resulting
//! `VirtualPairCol<F>` descriptors are exact for interactions that are affine
//! functions of columns (which covers all current bus interactions).

use p3_air::Air;
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use super::debug::{
    ChipRecord, RecordedInteraction, evaluate_chip_with_preprocessed_and_public_values,
};
use super::interaction::{ColumnRef, Interaction, InteractionDirection, VirtualPairCol};

/// Minimum trace height for local + next row evaluation.
const PROBE_HEIGHT: usize = 2;

/// Extract static interaction descriptors from a chip by column-scanning.
///
/// Evaluates the chip on a minimal (2-row) probe trace. Each column is set to `1`
/// in turn to determine its contribution to each interaction field.
///
/// Returns `(sends, receives)` as static [`Interaction<BabyBear>`] descriptors.
///
/// # Arguments
///
/// * `air` — The AIR chip to extract interactions from.
/// * `main_width` — Width of the main trace.
/// * `preprocessed` — Optional preprocessed trace (e.g. for PoseidonChip).
/// * `num_public_values` — Number of public value elements.
pub fn extract_interactions<A>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    num_public_values: usize,
) -> (Vec<Interaction<BabyBear>>, Vec<Interaction<BabyBear>>)
where
    A: for<'a> Air<super::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let pvs = vec![BabyBear::ZERO; num_public_values];

    // Phase 1: Evaluate with all-zero trace to establish baseline.
    let (baseline_sends, baseline_receives) = eval_baseline(air, main_width, preprocessed, &pvs);

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

/// Phase 1: Evaluate chip on an all-zero trace to establish baseline interaction counts.
///
/// Returns the per-row baseline sends and receives (first half of the 2-row evaluation).
fn eval_baseline<A>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    pvs: &[BabyBear],
) -> (
    Vec<RecordedInteraction<BabyBear>>,
    Vec<RecordedInteraction<BabyBear>>,
)
where
    A: for<'a> Air<super::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let zero_trace =
        RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * PROBE_HEIGHT], main_width);

    let baseline_record =
        evaluate_chip_with_preprocessed_and_public_values("_extract", air, &zero_trace, preprocessed, pvs)
            .expect("zero-trace evaluation should not fail constraints (extraction ignores constraint failures)");

    partition_per_row(&baseline_record)
}

/// Phase 2: Build initial [`Interaction`] descriptors from baseline constants.
///
/// Each baseline interaction becomes an `Interaction` with empty column weights
/// and the baseline field values as constants.
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
            kind: bi.kind,
            direction,
        })
        .collect()
}

/// Phase 3: Probe each column to determine its linear contribution to each interaction.
///
/// For each main column (local row) and next-row column, sets it to `1`, re-evaluates,
/// and records the delta from baseline as a column weight in the descriptors.
fn scan_column_contributions<A>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    pvs: &[BabyBear],
    baseline_sends: &[RecordedInteraction<BabyBear>],
    baseline_receives: &[RecordedInteraction<BabyBear>],
    send_descriptors: &mut [Interaction<BabyBear>],
    recv_descriptors: &mut [Interaction<BabyBear>],
) where
    A: for<'a> Air<super::debug::DebugConstraintBuilder<'a, BabyBear>>,
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
fn probe_single_column<A>(
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
    A: for<'a> Air<super::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let mut probe_data = vec![BabyBear::ZERO; main_width * PROBE_HEIGHT];
    probe_data[probe_index] = BabyBear::ONE;
    let probe_trace = RowMajorMatrix::new(probe_data, main_width);

    let probe_record = match evaluate_chip_with_preprocessed_and_public_values(
        "_extract",
        air,
        &probe_trace,
        preprocessed,
        pvs,
    ) {
        Ok(r) => r,
        Err(_) => return, // Skip columns that cause constraint failures.
    };

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
///
/// For each interaction field, if the probed value differs from the baseline,
/// record the delta as a column weight for `col_ref`.
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
///
/// For a 2-row probe trace, the debug evaluator produces interactions for both rows.
/// This returns only the first half (row 0) for each direction.
fn partition_per_row(
    record: &ChipRecord<BabyBear>,
) -> (
    Vec<RecordedInteraction<BabyBear>>,
    Vec<RecordedInteraction<BabyBear>>,
) {
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

/// Count the number of send and receive interactions per row for a chip.
///
/// Uses a minimal 2-row zero trace. Cheaper than full extraction.
pub fn count_interactions<A>(
    air: &A,
    main_width: usize,
    preprocessed: Option<&RowMajorMatrix<BabyBear>>,
    num_public_values: usize,
) -> (usize, usize)
where
    A: for<'a> Air<super::debug::DebugConstraintBuilder<'a, BabyBear>>,
{
    let zero_trace =
        RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * PROBE_HEIGHT], main_width);
    let pvs = vec![BabyBear::ZERO; num_public_values];

    let record = match evaluate_chip_with_preprocessed_and_public_values(
        "_count",
        air,
        &zero_trace,
        preprocessed,
        &pvs,
    ) {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::chips::range_check::RangeCheckChip;
    use crate::air::interaction::InteractionKind;
    use p3_air::BaseAir;

    #[test]
    fn count_range_check_interactions() {
        let chip = RangeCheckChip;
        let width = <RangeCheckChip as BaseAir<BabyBear>>::width(&chip);
        let (sends, receives) = count_interactions(&chip, width, None, 0);
        assert_eq!(sends, 0, "RangeCheck should have no sends");
        assert!(receives > 0, "RangeCheck should have receives");
    }

    #[test]
    fn extract_range_check_interactions() {
        let chip = RangeCheckChip;
        let width = <RangeCheckChip as BaseAir<BabyBear>>::width(&chip);
        let (sends, receives) = extract_interactions(&chip, width, None, 0);
        assert!(sends.is_empty(), "RangeCheck should have no sends");
        assert!(!receives.is_empty(), "RangeCheck should have receives");

        // Each receive should have a multiplicity and 1 value (the range check value).
        for recv in &receives {
            assert_eq!(recv.kind, InteractionKind::RangeCheck);
            assert_eq!(recv.values.len(), 1);
            assert_eq!(recv.direction, InteractionDirection::Receive);
        }
    }
}
