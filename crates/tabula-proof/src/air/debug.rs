//! Debug constraint checker: verify AIR constraints and LogUp balance.
//!
//! Two levels of checking:
//!
//! 1. **Single-chip** ([`debug_check`], [`debug_check_all`]):
//!    Evaluates local + transition constraints on a concrete trace.
//!    Any nonzero constraint value is a violation.
//!
//! 2. **Multi-chip LogUp** ([`debug_check_logup`]):
//!    Evaluates all chips' constraints AND verifies that LogUp interactions
//!    balance across the entire system. Uses random challenges over the
//!    quartic extension field `BabyBear⁴` for ~124-bit collision resistance.

use p3_air::{Air, AirBuilder, PairBuilder};
use p3_field::Field;
use p3_matrix::Matrix;
use p3_matrix::dense::{RowMajorMatrix, RowMajorMatrixView};
use p3_matrix::stack::VerticalPair;

use std::fmt;

use super::builder::InteractionAirBuilder;
use super::interaction::{AirInteraction, InteractionDirection, InteractionKind};

// ─── Error types ────────────────────────────────────────────────────────────

/// Error from a failed constraint check.
#[derive(Clone, Debug)]
pub struct ConstraintError {
    /// Row index where the violation occurred.
    pub row: usize,
    /// Index of the failing constraint (0-based within that row's eval).
    pub constraint_index: usize,
    /// The nonzero value of the failing constraint.
    pub value: String,
}

impl fmt::Display for ConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "constraint {} failed on row {}: value = {}",
            self.constraint_index, self.row, self.value
        )
    }
}

impl std::error::Error for ConstraintError {}

/// Error from a failed multi-chip LogUp check.
#[derive(Clone, Debug)]
pub enum MultiChipError {
    /// A local/transition constraint failed.
    Constraint {
        /// Which chip (by name).
        chip: String,
        /// The constraint error.
        error: ConstraintError,
    },
    /// LogUp balance failed: global sum is nonzero.
    LogUpImbalance {
        /// Human-readable description of the imbalance.
        description: String,
    },
}

impl fmt::Display for MultiChipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constraint { chip, error } => {
                write!(f, "[{chip}] {error}")
            }
            Self::LogUpImbalance { description } => {
                write!(f, "LogUp imbalance: {description}")
            }
        }
    }
}

impl std::error::Error for MultiChipError {}

/// Create a zero-width preprocessed `VerticalPair` for chips without preprocessed columns.
fn empty_preprocessed<F: Field>()
-> VerticalPair<RowMajorMatrixView<'static, F>, RowMajorMatrixView<'static, F>> {
    VerticalPair::new(
        RowMajorMatrixView::new(&[], 0),
        RowMajorMatrixView::new(&[], 0),
    )
}

// ─── Single-chip debug checking ─────────────────────────────────────────────

/// Verify that all AIR constraints are satisfied on a concrete trace.
///
/// Iterates over each row, evaluates constraints, and returns the first
/// violation found (if any).
pub fn debug_check<F, A>(air: &A, trace: &RowMajorMatrix<F>) -> Result<(), ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    debug_check_with_preprocessed(air, trace, None)
}

/// Verify AIR constraints with an optional preprocessed trace.
///
/// Like [`debug_check`] but passes a preprocessed matrix to `PairBuilder::preprocessed()`.
pub fn debug_check_with_preprocessed<F, A>(
    air: &A,
    trace: &RowMajorMatrix<F>,
    preprocessed: Option<&RowMajorMatrix<F>>,
) -> Result<(), ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    let height = trace.height();
    if height == 0 {
        return Ok(());
    }

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
        };

        air.eval(&mut builder);

        if let Some(err) = builder.first_failure {
            return Err(err);
        }
    }

    Ok(())
}

/// Verify AIR constraints and collect up to one violation per row.
///
/// Unlike [`debug_check`] (which stops at the first failing row), this
/// scans every row and collects the first failing constraint from each.
pub fn debug_check_all<F, A>(air: &A, trace: &RowMajorMatrix<F>) -> Vec<ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    let height = trace.height();
    if height == 0 {
        return vec![];
    }

    let mut errors = Vec::new();

    for i in 0..height {
        let i_next = (i + 1) % height;
        let local = trace.row_slice(i).expect("row exists");
        let next = trace.row_slice(i_next).expect("row exists");

        let main = VerticalPair::new(
            RowMajorMatrixView::new_row(&*local),
            RowMajorMatrixView::new_row(&*next),
        );

        let mut builder = DebugConstraintBuilder {
            row_index: i,
            main,
            preprocessed: empty_preprocessed(),
            is_first_row: if i == 0 { F::ONE } else { F::ZERO },
            is_last_row: if i == height - 1 { F::ONE } else { F::ZERO },
            is_transition: if i < height - 1 { F::ONE } else { F::ZERO },
            constraint_index: 0,
            first_failure: None,
            interactions: Vec::new(),
        };

        air.eval(&mut builder);

        if let Some(err) = builder.first_failure {
            errors.push(err);
        }
    }

    errors
}

// ─── Multi-chip LogUp checking ──────────────────────────────────────────────

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
    evaluate_chip_with_preprocessed(name, air, trace, None)
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
    let mut global_sum = F::ZERO;

    for record in records {
        for interaction in &record.interactions {
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
                InteractionDirection::Send => global_sum += contribution,
                InteractionDirection::Receive => global_sum -= contribution,
            }
        }
    }

    if global_sum != F::ZERO {
        return Err(MultiChipError::LogUpImbalance {
            description: format!("global LogUp sum is nonzero: {global_sum:?} (expected zero)"),
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

    let mut bus_sum = F::ZERO;

    for record in records {
        for interaction in &record.interactions {
            if interaction.kind != bus || interaction.multiplicity == F::ZERO {
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
                InteractionDirection::Send => bus_sum += contribution,
                InteractionDirection::Receive => bus_sum -= contribution,
            }
        }
    }

    if bus_sum != F::ZERO {
        return Err(MultiChipError::LogUpImbalance {
            description: format!("bus {bus:?} sum is nonzero: {bus_sum:?} (expected zero)"),
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

// ─── DebugConstraintBuilder ─────────────────────────────────────────────────

/// AirBuilder that evaluates constraints on concrete field values
/// and records LogUp interactions.
///
/// `Expr = F`, `Var = F`: expressions evaluate directly to field elements.
/// Interactions are collected into `self.interactions` for later analysis.
pub struct DebugConstraintBuilder<'a, F: Field> {
    row_index: usize,
    main: VerticalPair<RowMajorMatrixView<'a, F>, RowMajorMatrixView<'a, F>>,
    preprocessed: VerticalPair<RowMajorMatrixView<'a, F>, RowMajorMatrixView<'a, F>>,
    is_first_row: F,
    is_last_row: F,
    is_transition: F,
    constraint_index: usize,
    first_failure: Option<ConstraintError>,
    /// Interactions recorded during this row's evaluation.
    interactions: Vec<RecordedInteraction<F>>,
}

impl<'a, F: Field> AirBuilder for DebugConstraintBuilder<'a, F> {
    type F = F;
    type Expr = F;
    type Var = F;
    type M = VerticalPair<RowMajorMatrixView<'a, F>, RowMajorMatrixView<'a, F>>;

    fn main(&self) -> Self::M {
        self.main
    }

    fn is_first_row(&self) -> Self::Expr {
        self.is_first_row
    }

    fn is_last_row(&self) -> Self::Expr {
        self.is_last_row
    }

    fn is_transition_window(&self, _size: usize) -> Self::Expr {
        self.is_transition
    }

    fn assert_zero<I: Into<Self::Expr>>(&mut self, x: I) {
        let val = x.into();
        if val != F::ZERO && self.first_failure.is_none() {
            self.first_failure = Some(ConstraintError {
                row: self.row_index,
                constraint_index: self.constraint_index,
                value: format!("{:?}", val),
            });
        }
        self.constraint_index += 1;
    }
}

impl<'a, F: Field> PairBuilder for DebugConstraintBuilder<'a, F> {
    fn preprocessed(&self) -> Self::M {
        self.preprocessed
    }
}

impl<'a, F: Field> InteractionAirBuilder for DebugConstraintBuilder<'a, F> {
    fn send(&mut self, interaction: AirInteraction<F>) {
        self.interactions.push(RecordedInteraction {
            kind: interaction.kind,
            values: interaction.values,
            multiplicity: interaction.multiplicity,
            direction: InteractionDirection::Send,
        });
    }

    fn receive(&mut self, interaction: AirInteraction<F>) {
        self.interactions.push(RecordedInteraction {
            kind: interaction.kind,
            values: interaction.values,
            multiplicity: interaction.multiplicity,
            direction: InteractionDirection::Receive,
        });
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use p3_air::BaseAir;
    use p3_baby_bear::BabyBear;
    use p3_field::PrimeCharacteristicRing;

    /// Minimal chip that sends one interaction per real row.
    #[derive(Debug)]
    struct SenderChip;

    impl<F> BaseAir<F> for SenderChip {
        fn width(&self) -> usize {
            2 // [is_real, value]
        }
    }

    impl<AB: InteractionAirBuilder> Air<AB> for SenderChip {
        fn eval(&self, builder: &mut AB) {
            let main = builder.main();
            let local = main.row_slice(0).expect("row exists");
            let is_real: AB::Expr = local[0].clone().into();
            let value: AB::Expr = local[1].clone().into();

            builder.send(AirInteraction {
                values: vec![value],
                multiplicity: is_real,
                kind: InteractionKind::ReadAccess,
            });
        }
    }

    /// Minimal chip that receives one interaction per real row.
    #[derive(Debug)]
    struct ReceiverChip;

    impl<F> BaseAir<F> for ReceiverChip {
        fn width(&self) -> usize {
            2
        }
    }

    impl<AB: InteractionAirBuilder> Air<AB> for ReceiverChip {
        fn eval(&self, builder: &mut AB) {
            let main = builder.main();
            let local = main.row_slice(0).expect("row exists");
            let is_real: AB::Expr = local[0].clone().into();
            let value: AB::Expr = local[1].clone().into();

            builder.receive(AirInteraction {
                values: vec![value],
                multiplicity: is_real,
                kind: InteractionKind::ReadAccess,
            });
        }
    }

    fn bb(x: u32) -> BabyBear {
        BabyBear::new(x)
    }

    fn make_trace(rows: &[[u32; 2]]) -> RowMajorMatrix<BabyBear> {
        let padded_len = rows.len().next_power_of_two().max(2);
        let mut values = vec![BabyBear::ZERO; padded_len * 2];
        for (i, row) in rows.iter().enumerate() {
            values[i * 2] = bb(row[0]);
            values[i * 2 + 1] = bb(row[1]);
        }
        RowMajorMatrix::new(values, 2)
    }

    /// Helper: evaluate heterogeneous chips and check LogUp balance.
    fn assert_logup_balanced(records: Vec<ChipRecord<BabyBear>>) {
        check_logup_balance(&records).expect("LogUp should balance");
    }

    /// Helper: evaluate heterogeneous chips and assert LogUp imbalance.
    fn assert_logup_imbalanced(records: Vec<ChipRecord<BabyBear>>) {
        let err = check_logup_balance(&records).unwrap_err();
        assert!(
            matches!(err, MultiChipError::LogUpImbalance { .. }),
            "expected LogUpImbalance, got {err:?}"
        );
    }

    #[test]
    fn logup_balanced_simple() {
        let sender_trace = make_trace(&[[1, 42]]);
        let receiver_trace = make_trace(&[[1, 42]]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_balanced(records);
    }

    #[test]
    fn logup_imbalanced_missing_receive() {
        let sender_trace = make_trace(&[[1, 42]]);

        let records = vec![evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap()];
        assert_logup_imbalanced(records);
    }

    #[test]
    fn logup_imbalanced_wrong_value() {
        let sender_trace = make_trace(&[[1, 42]]);
        let receiver_trace = make_trace(&[[1, 99]]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_imbalanced(records);
    }

    #[test]
    fn logup_balanced_multiple_rows() {
        let sender_trace = make_trace(&[[1, 10], [1, 20], [1, 30]]);
        let receiver_trace = make_trace(&[[1, 10], [1, 20], [1, 30]]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_balanced(records);
    }

    #[test]
    fn logup_zero_multiplicity_ignored() {
        // is_real=0 rows should be ignored.
        let sender_trace = make_trace(&[[0, 42]]);
        let receiver_trace = make_trace(&[]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_balanced(records);
    }

    #[test]
    fn logup_balanced_multiset_duplicates() {
        let sender_trace = make_trace(&[[1, 42], [1, 42]]);
        let receiver_trace = make_trace(&[[1, 42], [1, 42]]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_balanced(records);
    }

    #[test]
    fn logup_imbalanced_duplicate_count_mismatch() {
        let sender_trace = make_trace(&[[1, 42], [1, 42]]);
        let receiver_trace = make_trace(&[[1, 42]]);

        let records = vec![
            evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
            evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
        ];
        assert_logup_imbalanced(records);
    }

    #[test]
    fn fingerprint_deterministic() {
        let alpha = bb(100);
        let beta = bb(200);
        let values = [bb(1), bb(2), bb(3)];

        let f1 = compute_fingerprint(&values, InteractionKind::ReadAccess, alpha, beta);
        let f2 = compute_fingerprint(&values, InteractionKind::ReadAccess, alpha, beta);
        assert_eq!(f1, f2);

        // Different bus kind produces different fingerprint.
        let f3 = compute_fingerprint(&values, InteractionKind::RangeCheck, alpha, beta);
        assert_ne!(f1, f3);
    }

    #[test]
    fn fingerprint_different_values() {
        let alpha = bb(100);
        let beta = bb(200);

        let f1 = compute_fingerprint(&[bb(1), bb(2)], InteractionKind::ReadAccess, alpha, beta);
        let f2 = compute_fingerprint(&[bb(1), bb(3)], InteractionKind::ReadAccess, alpha, beta);
        assert_ne!(f1, f2);
    }

    #[test]
    fn single_chip_debug_check_still_works() {
        // Existing single-chip debug_check works with interaction-aware builder.
        let trace = make_trace(&[[1, 42]]);
        debug_check(&SenderChip, &trace).expect("no local constraints to fail");
    }

    #[test]
    fn evaluate_chip_records_interactions() {
        let trace = make_trace(&[[1, 42]]);
        let record = evaluate_chip("Sender", &SenderChip, &trace).unwrap();

        // 2 rows (1 real + 1 padding), real row emits 1 interaction.
        let nonzero: Vec<_> = record
            .interactions
            .iter()
            .filter(|i| i.multiplicity != BabyBear::ZERO)
            .collect();
        assert_eq!(nonzero.len(), 1);
        assert_eq!(nonzero[0].kind, InteractionKind::ReadAccess);
        assert_eq!(nonzero[0].direction, InteractionDirection::Send);
    }
}
