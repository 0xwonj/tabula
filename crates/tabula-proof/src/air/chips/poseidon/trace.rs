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

#[cfg(test)]
mod tests {
    use p3_baby_bear::default_babybear_poseidon2_16;
    use p3_symmetric::Permutation;

    use super::*;
    use crate::air::debug::debug_check;

    use super::super::air::PoseidonChip;

    fn test_input(seed: u32) -> [BabyBear; WIDTH] {
        core::array::from_fn(|i| BabyBear::new(seed + i as u32))
    }

    // ── Valid traces ──

    #[test]
    fn valid_single_permutation() {
        let inputs = vec![test_input(1)];
        let trace = generate_poseidon_trace(&inputs);
        debug_check(&PoseidonChip, &trace).expect("single permutation should pass");
    }

    #[test]
    fn valid_two_permutations() {
        let inputs = vec![test_input(1), test_input(100)];
        let trace = generate_poseidon_trace(&inputs);
        debug_check(&PoseidonChip, &trace).expect("two permutations should pass");
    }

    #[test]
    fn valid_all_padding() {
        let inputs: Vec<[BabyBear; WIDTH]> = vec![];
        let trace = generate_poseidon_trace(&inputs);
        debug_check(&PoseidonChip, &trace).expect("all-padding should pass");
    }

    #[test]
    fn valid_zero_input() {
        let inputs = vec![[BabyBear::ZERO; WIDTH]];
        let trace = generate_poseidon_trace(&inputs);
        debug_check(&PoseidonChip, &trace).expect("zero input should pass");
    }

    #[test]
    fn trace_output_matches_p3() {
        let p3_perm = default_babybear_poseidon2_16();
        let input = test_input(42);
        let mut expected = input;
        p3_perm.permute_mut(&mut expected);

        let (rounds, output) = poseidon2_permutation(input);
        assert_eq!(output, expected, "permutation output mismatch");
        assert_eq!(rounds.len(), TOTAL_ROUNDS);
    }

    // ── Invalid traces ──

    #[test]
    fn invalid_corrupted_sbox_y2() {
        let inputs = vec![test_input(1)];
        let mut trace = generate_poseidon_trace(&inputs);

        // Corrupt sbox_y2[0] of round 0
        let width = poseidon_width();
        let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
        cols.sbox_y2[0] = BabyBear::new(999);

        debug_check(&PoseidonChip, &trace).expect_err("corrupted sbox_y2 should fail");
    }

    #[test]
    fn invalid_corrupted_sbox_y3() {
        let inputs = vec![test_input(1)];
        let mut trace = generate_poseidon_trace(&inputs);

        let width = poseidon_width();
        let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
        cols.sbox_y3[0] = BabyBear::new(888);

        debug_check(&PoseidonChip, &trace).expect_err("corrupted sbox_y3 should fail");
    }

    #[test]
    fn invalid_corrupted_state_transition() {
        let inputs = vec![test_input(1)];
        let mut trace = generate_poseidon_trace(&inputs);

        // Corrupt state[0] of round 1 (should be the output of round 0's linear layer)
        let width = poseidon_width();
        let cols: &mut PoseidonCols<BabyBear> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols.state[0] = BabyBear::new(777);

        debug_check(&PoseidonChip, &trace).expect_err("corrupted state transition should fail");
    }

    #[test]
    fn invalid_corrupted_full_round_sbox() {
        let inputs = vec![test_input(1)];
        let mut trace = generate_poseidon_trace(&inputs);

        // Corrupt sbox_y2[5] of round 0 (a full round, element 5 should be constrained)
        let width = poseidon_width();
        let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
        cols.sbox_y2[5] = BabyBear::new(666);

        debug_check(&PoseidonChip, &trace)
            .expect_err("corrupted full-round sbox for element 5 should fail");
    }

    #[test]
    fn invalid_broken_round_counter() {
        let inputs = vec![test_input(1)];
        let mut trace = generate_poseidon_trace(&inputs);

        // Set round 1's counter to 5 instead of 1
        let width = poseidon_width();
        let cols: &mut PoseidonCols<BabyBear> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols.round_ctr = BabyBear::new(5);

        debug_check(&PoseidonChip, &trace).expect_err("wrong round counter should fail");
    }
}
