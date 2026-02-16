//! Debug constraint checker: verify AIR constraints against concrete traces.
//!
//! Evaluates all constraints at each row pair `(i, i+1)` on a concrete
//! `RowMajorMatrix<F>`. Any nonzero constraint value indicates a violation.
//!
//! This replaces `p3-uni-stark::check_constraints` for M6 (no prover needed).

use p3_air::{Air, AirBuilder};
use p3_field::Field;
use p3_matrix::Matrix;
use p3_matrix::dense::{RowMajorMatrix, RowMajorMatrixView};
use p3_matrix::stack::VerticalPair;

use std::fmt;

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

/// Verify that all AIR constraints are satisfied on a concrete trace.
///
/// Iterates over each row, evaluates constraints, and returns the first
/// violation found (if any).
pub fn debug_check<F, A>(air: &A, trace: &RowMajorMatrix<F>) -> Result<(), ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    let height = trace.height();
    if height == 0 {
        return Ok(());
    }

    for i in 0..height {
        // Cyclic: row height-1 wraps to row 0 (standard STARK trace semantics).
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
            is_first_row: if i == 0 { F::ONE } else { F::ZERO },
            is_last_row: if i == height - 1 { F::ONE } else { F::ZERO },
            is_transition: if i < height - 1 { F::ONE } else { F::ZERO },
            constraint_index: 0,
            first_failure: None,
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
/// Unlike `debug_check` (which stops at the first failing row), this function
/// scans every row and collects the first failing constraint from each row.
/// Useful for debugging when multiple rows fail simultaneously.
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
            is_first_row: if i == 0 { F::ONE } else { F::ZERO },
            is_last_row: if i == height - 1 { F::ONE } else { F::ZERO },
            is_transition: if i < height - 1 { F::ONE } else { F::ZERO },
            constraint_index: 0,
            first_failure: None,
        };

        air.eval(&mut builder);

        if let Some(err) = builder.first_failure {
            errors.push(err);
        }
    }

    errors
}

/// AirBuilder that evaluates constraints on concrete field values.
///
/// `Expr = F`, `Var = F`: expressions evaluate directly to field elements.
pub struct DebugConstraintBuilder<'a, F: Field> {
    row_index: usize,
    main: VerticalPair<RowMajorMatrixView<'a, F>, RowMajorMatrixView<'a, F>>,
    is_first_row: F,
    is_last_row: F,
    is_transition: F,
    constraint_index: usize,
    first_failure: Option<ConstraintError>,
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
