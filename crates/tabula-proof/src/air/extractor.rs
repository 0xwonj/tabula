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

use super::debug::{DebugConstraintBuilder, evaluate_chip_with_preprocessed_and_public_values};
use super::interaction::{ColumnRef, Interaction, InteractionDirection, VirtualPairCol};

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
    A: for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>,
{
    // Phase 1: Evaluate with all-zero trace to establish baseline.
    let height = 2; // Minimum for local + next row.
    let zero_trace = RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * height], main_width);
    let pvs = vec![BabyBear::ZERO; num_public_values];

    let baseline_record =
        evaluate_chip_with_preprocessed_and_public_values("_extract", air, &zero_trace, preprocessed, &pvs)
            .expect("zero-trace evaluation should not fail constraints (extraction ignores constraint failures)");

    // Partition baseline into sends and receives.
    let mut baseline_sends = Vec::new();
    let mut baseline_receives = Vec::new();
    for interaction in &baseline_record.interactions {
        // Only take row-0 interactions (skip row-1 duplicates).
        // The debug evaluator runs over all rows, producing 2x interactions
        // for a 2-row trace. We take the first half.
        match interaction.direction {
            InteractionDirection::Send => baseline_sends.push(interaction),
            InteractionDirection::Receive => baseline_receives.push(interaction),
        }
    }
    // For a 2-row trace, each row produces the same set of interactions.
    // Take the first half (row 0).
    let sends_per_row = baseline_sends.len() / height;
    let receives_per_row = baseline_receives.len() / height;
    let baseline_sends = &baseline_sends[..sends_per_row];
    let baseline_receives = &baseline_receives[..receives_per_row];

    // Phase 2: For each main column (local row), probe with column = 1.
    let mut send_descriptors: Vec<Interaction<BabyBear>> = baseline_sends
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
            direction: InteractionDirection::Send,
        })
        .collect();

    let mut recv_descriptors: Vec<Interaction<BabyBear>> = baseline_receives
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
            direction: InteractionDirection::Receive,
        })
        .collect();

    // Scan local columns.
    for col in 0..main_width {
        let mut probe_data = vec![BabyBear::ZERO; main_width * height];
        probe_data[col] = BabyBear::ONE; // Row 0, column `col`.
        let probe_trace = RowMajorMatrix::new(probe_data, main_width);

        let probe_record = match evaluate_chip_with_preprocessed_and_public_values(
            "_extract",
            air,
            &probe_trace,
            preprocessed,
            &pvs,
        ) {
            Ok(r) => r,
            Err(_) => continue, // Skip columns that cause constraint failures.
        };

        let mut probe_sends = Vec::new();
        let mut probe_receives = Vec::new();
        for interaction in &probe_record.interactions {
            match interaction.direction {
                InteractionDirection::Send => probe_sends.push(interaction),
                InteractionDirection::Receive => probe_receives.push(interaction),
            }
        }
        let probe_sends = &probe_sends[..sends_per_row.min(probe_sends.len())];
        let probe_receives = &probe_receives[..receives_per_row.min(probe_receives.len())];

        // Update send descriptors.
        for (i, (probed, baseline)) in probe_sends.iter().zip(baseline_sends.iter()).enumerate() {
            if i >= send_descriptors.len() {
                break;
            }
            for (j, (&pval, &bval)) in probed.values.iter().zip(baseline.values.iter()).enumerate()
            {
                let weight = pval - bval;
                if weight != BabyBear::ZERO && j < send_descriptors[i].values.len() {
                    send_descriptors[i].values[j]
                        .column_weights
                        .push((ColumnRef::Local(col), weight));
                }
            }
            let mult_weight = probed.multiplicity - baseline.multiplicity;
            if mult_weight != BabyBear::ZERO {
                send_descriptors[i]
                    .multiplicity
                    .column_weights
                    .push((ColumnRef::Local(col), mult_weight));
            }
        }

        // Update receive descriptors.
        for (i, (probed, baseline)) in probe_receives
            .iter()
            .zip(baseline_receives.iter())
            .enumerate()
        {
            if i >= recv_descriptors.len() {
                break;
            }
            for (j, (&pval, &bval)) in probed.values.iter().zip(baseline.values.iter()).enumerate()
            {
                let weight = pval - bval;
                if weight != BabyBear::ZERO && j < recv_descriptors[i].values.len() {
                    recv_descriptors[i].values[j]
                        .column_weights
                        .push((ColumnRef::Local(col), weight));
                }
            }
            let mult_weight = probed.multiplicity - baseline.multiplicity;
            if mult_weight != BabyBear::ZERO {
                recv_descriptors[i]
                    .multiplicity
                    .column_weights
                    .push((ColumnRef::Local(col), mult_weight));
            }
        }
    }

    // Scan next-row columns.
    for col in 0..main_width {
        let mut probe_data = vec![BabyBear::ZERO; main_width * height];
        probe_data[main_width + col] = BabyBear::ONE; // Row 1, column `col`.
        let probe_trace = RowMajorMatrix::new(probe_data, main_width);

        let probe_record = match evaluate_chip_with_preprocessed_and_public_values(
            "_extract",
            air,
            &probe_trace,
            preprocessed,
            &pvs,
        ) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut probe_sends = Vec::new();
        let mut probe_receives = Vec::new();
        for interaction in &probe_record.interactions {
            match interaction.direction {
                InteractionDirection::Send => probe_sends.push(interaction),
                InteractionDirection::Receive => probe_receives.push(interaction),
            }
        }
        let probe_sends = &probe_sends[..sends_per_row.min(probe_sends.len())];
        let probe_receives = &probe_receives[..receives_per_row.min(probe_receives.len())];

        for (i, (probed, baseline)) in probe_sends.iter().zip(baseline_sends.iter()).enumerate() {
            if i >= send_descriptors.len() {
                break;
            }
            for (j, (&pval, &bval)) in probed.values.iter().zip(baseline.values.iter()).enumerate()
            {
                let weight = pval - bval;
                if weight != BabyBear::ZERO && j < send_descriptors[i].values.len() {
                    send_descriptors[i].values[j]
                        .column_weights
                        .push((ColumnRef::Next(col), weight));
                }
            }
            let mult_weight = probed.multiplicity - baseline.multiplicity;
            if mult_weight != BabyBear::ZERO {
                send_descriptors[i]
                    .multiplicity
                    .column_weights
                    .push((ColumnRef::Next(col), mult_weight));
            }
        }

        for (i, (probed, baseline)) in probe_receives
            .iter()
            .zip(baseline_receives.iter())
            .enumerate()
        {
            if i >= recv_descriptors.len() {
                break;
            }
            for (j, (&pval, &bval)) in probed.values.iter().zip(baseline.values.iter()).enumerate()
            {
                let weight = pval - bval;
                if weight != BabyBear::ZERO && j < recv_descriptors[i].values.len() {
                    recv_descriptors[i].values[j]
                        .column_weights
                        .push((ColumnRef::Next(col), weight));
                }
            }
            let mult_weight = probed.multiplicity - baseline.multiplicity;
            if mult_weight != BabyBear::ZERO {
                recv_descriptors[i]
                    .multiplicity
                    .column_weights
                    .push((ColumnRef::Next(col), mult_weight));
            }
        }
    }

    (send_descriptors, recv_descriptors)
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
    A: for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>,
{
    let height = 2;
    let zero_trace = RowMajorMatrix::new(vec![BabyBear::ZERO; main_width * height], main_width);
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
    (sends / height, receives / height)
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
