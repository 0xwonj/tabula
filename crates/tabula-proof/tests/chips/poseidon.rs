use p3_baby_bear::{BabyBear, default_babybear_poseidon2_16};
use p3_field::PrimeCharacteristicRing;
use p3_symmetric::Permutation;

use tabula_proof::air::chips::poseidon::air::PoseidonChip;
use tabula_proof::air::chips::poseidon::columns::{POSEIDON_WIDTH, PoseidonCols, poseidon_width};
use tabula_proof::air::chips::poseidon::constants::{
    TOTAL_ROUNDS, WIDTH, internal_diag_minus_1, is_full_round, poseidon2_permutation,
    round_constants, sbox_with_intermediates,
};
use tabula_proof::air::chips::poseidon::trace::generate_poseidon_trace;
use tabula_proof::air::{borrow_cols_mut, debug_check};

use crate::common::builders::poseidon_test_input;

// ── Valid traces ──

#[test]
fn valid_single_permutation() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    debug_check(&PoseidonChip, &trace).expect("single permutation should pass");
}

#[test]
fn valid_two_permutations() {
    let inputs = vec![poseidon_test_input(1), poseidon_test_input(100)];
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
    let input = poseidon_test_input(42);
    let mut expected = input;
    p3_perm.permute_mut(&mut expected);

    let (rounds, output) = poseidon2_permutation(input);
    assert_eq!(output, expected, "permutation output mismatch");
    assert_eq!(rounds.len(), TOTAL_ROUNDS);
}

// ── Invalid traces ──

#[test]
fn invalid_corrupted_sbox_y2() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);

    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.sbox_y2[0] = BabyBear::new(999);

    debug_check(&PoseidonChip, &trace).expect_err("corrupted sbox_y2 should fail");
}

#[test]
fn invalid_corrupted_sbox_y3() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);

    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.sbox_y3[0] = BabyBear::new(888);

    debug_check(&PoseidonChip, &trace).expect_err("corrupted sbox_y3 should fail");
}

#[test]
fn invalid_corrupted_state_transition() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);

    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.state[0] = BabyBear::new(777);

    debug_check(&PoseidonChip, &trace).expect_err("corrupted state transition should fail");
}

#[test]
fn invalid_corrupted_full_round_sbox() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);

    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.sbox_y2[5] = BabyBear::new(666);

    debug_check(&PoseidonChip, &trace)
        .expect_err("corrupted full-round sbox for element 5 should fail");
}

#[test]
fn invalid_broken_round_counter() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);

    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.round_ctr = BabyBear::new(5);

    debug_check(&PoseidonChip, &trace).expect_err("wrong round counter should fail");
}

// ── Constants tests ──

#[test]
fn round_constants_count() {
    assert_eq!(TOTAL_ROUNDS, 21);
    for r in 0..TOTAL_ROUNDS {
        let rc = round_constants(r);
        if is_full_round(r) {
            assert!(rc.iter().any(|x| *x != BabyBear::ZERO));
        } else {
            for (i, val) in rc.iter().enumerate().skip(1) {
                assert_eq!(
                    *val,
                    BabyBear::ZERO,
                    "partial round {r} has nonzero rc[{i}]"
                );
            }
        }
    }
}

#[test]
fn internal_diag_nonzero() {
    let diag = internal_diag_minus_1();
    for (i, d) in diag.iter().enumerate() {
        assert_ne!(*d, BabyBear::ZERO, "diag[{i}] should be nonzero");
    }
}

#[test]
fn permutation_matches_p3_constants() {
    let p3_perm = default_babybear_poseidon2_16();

    let input: [BabyBear; 16] = core::array::from_fn(|i| BabyBear::new(i as u32 + 1));
    let mut p3_output = input;
    p3_perm.permute_mut(&mut p3_output);

    let (_, our_output) = poseidon2_permutation(input);
    assert_eq!(our_output, p3_output, "our Poseidon2 should match p3's");
}

#[test]
fn sbox_correct() {
    let x = BabyBear::new(42);
    let si = sbox_with_intermediates(x);
    assert_eq!(si.y, x);
    assert_eq!(si.y2, x * x);
    assert_eq!(si.y3, x * x * x);
    let expected = x * x * x * (x * x) * (x * x);
    assert_eq!(si.out, expected);
}

// ── Column width test ──

#[test]
fn width() {
    assert_eq!(POSEIDON_WIDTH, 69);
}
