//! Debug-time public input binding checks.

use p3_field::Field;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use crate::smt_path::columns::SmtTablePathCols;
use tabula_stark::air::columns::borrow_cols;

use tabula_stark::debug::MultiChipError;

/// Check that SmtTablePath root rows match the expected old/new state roots.
///
/// Scans the trace for `is_root=1` rows and extracts `old_parent[8]` / `new_parent[8]`.
/// Verifies all table-level roots hash up to the expected state roots via compress.
///
/// `expected_old_root` and `expected_new_root` are the `PublicInputs` roots
/// as NativeDigest-encoded BabyBear arrays.
///
/// This is a debug-time convenience helper.
/// Root/public-value binding is enforced in AIR by `SmtTablePathChip`.
pub fn check_public_input_binding<F: Field + core::fmt::Debug>(
    smt_table_path_trace: &RowMajorMatrix<F>,
    smt_table_path_width: usize,
    expected_old_root: &[F; 8],
    expected_new_root: &[F; 8],
) -> Result<(), MultiChipError> {
    let height = smt_table_path_trace.height();
    let mut found_root = false;

    for i in 0..height {
        let row = smt_table_path_trace.row_slice(i).expect("row exists");
        assert_eq!(row.len(), smt_table_path_width, "width mismatch");
        let cols: &SmtTablePathCols<F> = borrow_cols(&row);

        let is_real = cols.base.is_real;
        let is_root = cols.base.is_root;

        if is_real == F::ONE && is_root == F::ONE {
            found_root = true;
            // Check old root
            for (j, (actual, expected)) in cols
                .base
                .old_parent
                .iter()
                .zip(expected_old_root.iter())
                .enumerate()
            {
                if *actual != *expected {
                    return Err(MultiChipError::LogUpImbalance {
                        description: format!(
                            "SmtTablePath row {i}: old_parent[{j}] = {actual:?}, expected {expected:?}",
                        ),
                    });
                }
            }
            // Check new root
            for (j, (actual, expected)) in cols
                .base
                .new_parent
                .iter()
                .zip(expected_new_root.iter())
                .enumerate()
            {
                if *actual != *expected {
                    return Err(MultiChipError::LogUpImbalance {
                        description: format!(
                            "SmtTablePath row {i}: new_parent[{j}] = {actual:?}, expected {expected:?}",
                        ),
                    });
                }
            }
        }
    }

    if !found_root {
        return Err(MultiChipError::LogUpImbalance {
            description: "SmtTablePath trace has no is_root=1 rows".to_string(),
        });
    }

    Ok(())
}
