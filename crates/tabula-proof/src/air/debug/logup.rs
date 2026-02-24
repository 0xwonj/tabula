//! Multi-chip LogUp checking.
//!
//! Evaluates all chips' constraints AND verifies that LogUp interactions
//! balance across the entire system. Uses random challenges over the
//! base field for collision resistance.

use p3_air::Air;
use p3_field::Field;
use p3_matrix::Matrix;
use p3_matrix::dense::{RowMajorMatrix, RowMajorMatrixView};
use p3_matrix::stack::VerticalPair;

use crate::air::interaction::{InteractionDirection, InteractionKind};

use super::builder::DebugConstraintBuilder;
use super::errors::MultiChipError;
use super::single_chip::empty_preprocessed;

/// A concrete interaction recorded during debug evaluation.
#[derive(Clone, Debug)]
pub struct RecordedInteraction<F> {
    /// Which LogUp bus.
    pub kind: InteractionKind,
    /// Concrete tuple values.
    pub values: Vec<F>,
    /// Concrete multiplicity.
    pub multiplicity: F,
    /// Send or receive.
    pub direction: InteractionDirection,
}

/// Recorded interactions from evaluating a single chip.
///
/// Produced by [`evaluate_chip`]. Passed to [`check_logup_balance`].
#[derive(Clone)]
pub struct ChipRecord<F> {
    /// Human-readable chip name (for error messages).
    pub name: String,
    /// All interactions from all rows of this chip.
    pub interactions: Vec<RecordedInteraction<F>>,
}

/// A chip with its name and trace, for homogeneous multi-chip checking.
///
/// For heterogeneous chips, use [`evaluate_chip`] + [`check_logup_balance`] directly.
pub struct ChipTrace<'a, F: Field, A> {
    /// Human-readable chip name (for error messages).
    pub name: &'a str,
    /// The AIR chip.
    pub air: &'a A,
    /// The main trace matrix.
    pub trace: &'a RowMajorMatrix<F>,
}

// ── Evaluate chip variants ──────────────────────────────────────────────────

/// Evaluate a single chip's constraints and record all LogUp interactions.
///
/// Checks local constraints row-by-row. Returns a [`ChipRecord`] containing
/// all interactions emitted during evaluation. Fails early on the first
/// constraint violation.
pub fn evaluate_chip<F, A>(
    name: &str,
    air: &A,
    trace: &RowMajorMatrix<F>,
) -> Result<ChipRecord<F>, MultiChipError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    evaluate_chip_with_preprocessed_and_public_values(name, air, trace, None, &[])
}

/// Like [`evaluate_chip`] but with explicit public values.
pub fn evaluate_chip_with_public_values<F, A>(
    name: &str,
    air: &A,
    trace: &RowMajorMatrix<F>,
    public_values: &[F],
) -> Result<ChipRecord<F>, MultiChipError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    evaluate_chip_with_preprocessed_and_public_values(name, air, trace, None, public_values)
}

/// Like [`evaluate_chip`] but with an optional preprocessed trace.
pub fn evaluate_chip_with_preprocessed<F, A>(
    name: &str,
    air: &A,
    trace: &RowMajorMatrix<F>,
    preprocessed: Option<&RowMajorMatrix<F>>,
) -> Result<ChipRecord<F>, MultiChipError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    evaluate_chip_with_preprocessed_and_public_values(name, air, trace, preprocessed, &[])
}

/// Like [`evaluate_chip_with_preprocessed`] but also binds public values.
pub fn evaluate_chip_with_preprocessed_and_public_values<F, A>(
    name: &str,
    air: &A,
    trace: &RowMajorMatrix<F>,
    preprocessed: Option<&RowMajorMatrix<F>>,
    public_values: &[F],
) -> Result<ChipRecord<F>, MultiChipError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    let height = trace.height();
    let mut all_interactions = Vec::new();

    for i in 0..height {
        let i_next = (i + 1) % height;
        let local = trace.row_slice(i).expect("row exists");
        let next = trace.row_slice(i_next).expect("row exists");

        let main = VerticalPair::new(
            RowMajorMatrixView::new_row(&*local),
            RowMajorMatrixView::new_row(&*next),
        );

        // Bind preprocessed row slices at this scope level so they live long enough.
        let (prep_local_slice, prep_next_slice);
        let prep = if let Some(prep_trace) = preprocessed {
            prep_local_slice = prep_trace.row_slice(i).expect("preprocessed row exists");
            prep_next_slice = prep_trace
                .row_slice(i_next)
                .expect("preprocessed row exists");
            VerticalPair::new(
                RowMajorMatrixView::new_row(&*prep_local_slice),
                RowMajorMatrixView::new_row(&*prep_next_slice),
            )
        } else {
            empty_preprocessed()
        };

        let mut builder = DebugConstraintBuilder {
            row_index: i,
            main,
            preprocessed: prep,
            is_first_row: if i == 0 { F::ONE } else { F::ZERO },
            is_last_row: if i == height - 1 { F::ONE } else { F::ZERO },
            is_transition: if i < height - 1 { F::ONE } else { F::ZERO },
            constraint_index: 0,
            first_failure: None,
            interactions: Vec::new(),
            public_values,
        };

        air.eval(&mut builder);

        if let Some(err) = builder.first_failure {
            return Err(MultiChipError::Constraint {
                chip: name.to_string(),
                error: err,
            });
        }

        all_interactions.extend(builder.interactions);
    }

    Ok(ChipRecord {
        name: name.to_string(),
        interactions: all_interactions,
    })
}

// ── LogUp balance checking ──────────────────────────────────────────────────

/// Check that LogUp interactions balance across all chips.
///
/// Takes [`ChipRecord`]s from [`evaluate_chip`] and verifies that the
/// global LogUp sum is zero (sends = receives for every bus).
///
/// Uses deterministic challenges for reproducibility.
pub fn check_logup_balance<F: Field>(records: &[ChipRecord<F>]) -> Result<(), MultiChipError> {
    let alpha = F::from_u64(0x1234_5678_9ABC_DEF0);
    let beta = F::from_u64(0xFEDC_BA98_7654_3210);
    check_logup_balance_with_challenges(records, alpha, beta)
}

/// Like [`check_logup_balance`] but with explicit challenges.
pub fn check_logup_balance_with_challenges<F: Field>(
    records: &[ChipRecord<F>],
    alpha: F,
    beta: F,
) -> Result<(), MultiChipError> {
    let sum = accumulate_logup_sum(records, None, alpha, beta)?;
    if sum != F::ZERO {
        return Err(MultiChipError::LogUpImbalance {
            description: format!("global LogUp sum is nonzero: {sum:?} (expected zero)"),
        });
    }
    Ok(())
}

/// Check LogUp balance for a specific bus only, ignoring all other buses.
///
/// Useful for isolated bus tests where only one bus should be verified
/// without requiring all other buses to be balanced.
pub fn check_bus_balance<F: Field>(
    records: &[ChipRecord<F>],
    bus: InteractionKind,
) -> Result<(), MultiChipError> {
    let alpha = F::from_u64(0x1234_5678_9ABC_DEF0);
    let beta = F::from_u64(0xFEDC_BA98_7654_3210);
    let sum = accumulate_logup_sum(records, Some(bus), alpha, beta)?;
    if sum != F::ZERO {
        return Err(MultiChipError::LogUpImbalance {
            description: format!("bus {bus:?} sum is nonzero: {sum:?} (expected zero)"),
        });
    }
    Ok(())
}

/// Convenience: evaluate multiple chips of the same type and check LogUp balance.
///
/// For heterogeneous chip types, use [`evaluate_chip`] + [`check_logup_balance`].
pub fn debug_check_logup<F, A>(chips: &[ChipTrace<'_, F, A>]) -> Result<(), MultiChipError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    let mut records = Vec::with_capacity(chips.len());
    for chip in chips {
        records.push(evaluate_chip(chip.name, chip.air, chip.trace)?);
    }
    check_logup_balance(&records)
}

// ── Fingerprint ─────────────────────────────────────────────────────────────

/// Compute the RLC fingerprint for an interaction tuple.
///
/// `f = α + β⁰ · kind_tag + β¹ · values[0] + β² · values[1] + …`
pub fn compute_fingerprint<F: Field>(values: &[F], kind: InteractionKind, alpha: F, beta: F) -> F {
    let mut result = alpha + F::from_u64(kind.tag() as u64);
    let mut beta_power = beta;
    for val in values {
        result += beta_power * *val;
        beta_power *= beta;
    }
    result
}

/// Accumulate the LogUp sum across all chip records, optionally filtering by bus.
fn accumulate_logup_sum<F: Field>(
    records: &[ChipRecord<F>],
    bus_filter: Option<InteractionKind>,
    alpha: F,
    beta: F,
) -> Result<F, MultiChipError> {
    let mut sum = F::ZERO;

    for record in records {
        for interaction in &record.interactions {
            if let Some(bus) = bus_filter {
                if interaction.kind != bus {
                    continue;
                }
            }
            if interaction.multiplicity == F::ZERO {
                continue;
            }

            let fingerprint =
                compute_fingerprint(&interaction.values, interaction.kind, alpha, beta);

            if fingerprint == F::ZERO {
                return Err(MultiChipError::LogUpImbalance {
                    description: format!(
                        "zero fingerprint in chip '{}' for bus {:?}",
                        record.name, interaction.kind
                    ),
                });
            }

            let contribution = interaction.multiplicity / fingerprint;
            match interaction.direction {
                InteractionDirection::Send => sum += contribution,
                InteractionDirection::Receive => sum -= contribution,
            }
        }
    }

    Ok(sum)
}
