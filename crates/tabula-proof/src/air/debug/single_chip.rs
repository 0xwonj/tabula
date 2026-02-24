//! Single-chip debug constraint checking.
//!
//! Evaluates local + transition constraints on a concrete trace.
//! Any nonzero constraint value is a violation.

use p3_air::Air;
use p3_field::Field;
use p3_matrix::Matrix;
use p3_matrix::dense::{RowMajorMatrix, RowMajorMatrixView};
use p3_matrix::stack::VerticalPair;

use super::builder::DebugConstraintBuilder;
use super::errors::ConstraintError;

/// Create a zero-width preprocessed `VerticalPair` for chips without preprocessed columns.
pub(super) fn empty_preprocessed<F: Field>()
-> VerticalPair<RowMajorMatrixView<'static, F>, RowMajorMatrixView<'static, F>> {
    VerticalPair::new(
        RowMajorMatrixView::new(&[], 0),
        RowMajorMatrixView::new(&[], 0),
    )
}

/// Verify that all AIR constraints are satisfied on a concrete trace.
///
/// Iterates over each row, evaluates constraints, and returns the first
/// violation found (if any).
pub fn debug_check<F, A>(air: &A, trace: &RowMajorMatrix<F>) -> Result<(), ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    debug_check_with_preprocessed_and_public_values(air, trace, None, &[])
}

/// Verify AIR constraints with explicit public values.
pub fn debug_check_with_public_values<F, A>(
    air: &A,
    trace: &RowMajorMatrix<F>,
    public_values: &[F],
) -> Result<(), ConstraintError>
where
    F: Field,
    A: for<'a> Air<DebugConstraintBuilder<'a, F>>,
{
    debug_check_with_preprocessed_and_public_values(air, trace, None, public_values)
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
    debug_check_with_preprocessed_and_public_values(air, trace, preprocessed, &[])
}

/// Like [`debug_check_with_preprocessed`] but also binds public values.
pub fn debug_check_with_preprocessed_and_public_values<F, A>(
    air: &A,
    trace: &RowMajorMatrix<F>,
    preprocessed: Option<&RowMajorMatrix<F>>,
    public_values: &[F],
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
            public_values,
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
            public_values: &[],
        };

        air.eval(&mut builder);

        if let Some(err) = builder.first_failure {
            errors.push(err);
        }
    }

    errors
}
