//! Trace generation for the PoseidonChip.
//!
//! Converts a list of permutation inputs into a `RowMajorMatrix<BabyBear>` trace
//! with 21 rows per permutation (one per round).

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{PoseidonCols, poseidon_width};
use super::constants::{TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation};

/// Generate a Poseidon2 trace from a list of permutation inputs.
///
/// Each input is a 16-element BabyBear vector. The trace has 21 rows per
/// permutation, padded to a power of 2.
pub fn generate_poseidon_trace(inputs: &[[BabyBear; WIDTH]]) -> RowMajorMatrix<BabyBear> {
    let width = poseidon_width();
    let num_real = inputs.len() * TOTAL_ROUNDS;
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    for (perm_idx, input) in inputs.iter().enumerate() {
        let (rounds, _output) = poseidon2_permutation(*input);
        debug_assert_eq!(rounds.len(), TOTAL_ROUNDS);

        for (r, round_data) in rounds.iter().enumerate() {
            let row_idx = perm_idx * TOTAL_ROUNDS + r;
            let offset = row_idx * width;
            let cols: &mut PoseidonCols<BabyBear> =
                borrow_cols_mut(&mut values[offset..offset + width]);

            cols.is_real = BabyBear::ONE;
            cols.state = round_data.state_before;
            cols.rc = round_data.rc;
            cols.sbox_y2 = round_data.sbox_y2;
            cols.sbox_y3 = round_data.sbox_y3;
            cols.round_ctr = BabyBear::new(r as u32);
            cols.is_full_round = bool_fe(is_full_round(r));
            cols.is_first_round = bool_fe(r == 0);
            cols.is_last_round = bool_fe(r == TOTAL_ROUNDS - 1);
        }
    }

    RowMajorMatrix::new(values, width)
}
