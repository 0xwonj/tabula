use p3_baby_bear::{BabyBear, default_babybear_poseidon2_16};
use p3_field::PrimeCharacteristicRing;
use p3_symmetric::Permutation;

use tabula_proof::air::borrow_cols_mut;
use tabula_proof::chips::poseidon::air::PoseidonChip;
use tabula_proof::chips::poseidon::columns::{
    POSEIDON_PREPROCESSED_WIDTH, POSEIDON_WIDTH, PoseidonCols, PoseidonPreprocessedCols,
    poseidon_width,
};
use tabula_proof::chips::poseidon::constants::{
    TOTAL_ROUNDS, WIDTH, internal_diag_minus_1, is_full_round, poseidon2_permutation,
    round_constants, sbox_with_intermediates,
};
use tabula_proof::chips::poseidon::trace::{
    generate_poseidon_preprocessed, generate_poseidon_trace,
};
use tabula_proof::debug::{debug_check, debug_check_with_preprocessed};

use crate::common::builders::poseidon_test_input;

// ── Valid traces (backward-compatible, no preprocessed) ──

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

// ── Valid traces WITH preprocessed ──

#[test]
fn valid_with_preprocessed() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    let prep = generate_poseidon_preprocessed(inputs.len());
    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect("valid trace with preprocessed should pass");
}

#[test]
fn valid_two_permutations_with_preprocessed() {
    let inputs = vec![poseidon_test_input(1), poseidon_test_input(100)];
    let trace = generate_poseidon_trace(&inputs);
    let prep = generate_poseidon_preprocessed(inputs.len());
    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect("two permutations with preprocessed should pass");
}

// ── Invalid traces (preprocessed RC verification) ──

#[test]
fn invalid_corrupted_main_rc_vs_preprocessed() {
    let inputs = vec![poseidon_test_input(1)];
    let mut trace = generate_poseidon_trace(&inputs);
    let prep = generate_poseidon_preprocessed(inputs.len());

    // Corrupt rc[0] on main trace row 0 → mismatch with preprocessed
    let width = poseidon_width();
    let cols: &mut PoseidonCols<BabyBear> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.rc[0] += BabyBear::ONE;

    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect_err("main trace rc mismatch with preprocessed should fail");
}

#[test]
fn invalid_corrupted_preprocessed_rc() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    let mut prep = generate_poseidon_preprocessed(inputs.len());

    // Corrupt preprocessed rc[0] on row 0
    let cols: &mut PoseidonPreprocessedCols<BabyBear> =
        borrow_cols_mut(&mut prep.values[0..POSEIDON_PREPROCESSED_WIDTH]);
    cols.rc[0] += BabyBear::ONE;

    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect_err("corrupted preprocessed rc should fail");
}

#[test]
fn invalid_wrong_is_full_round_preprocessed() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    let mut prep = generate_poseidon_preprocessed(inputs.len());

    // Flip is_full_round on preprocessed row 0 (which IS a full round)
    let cols: &mut PoseidonPreprocessedCols<BabyBear> =
        borrow_cols_mut(&mut prep.values[0..POSEIDON_PREPROCESSED_WIDTH]);
    cols.is_full_round = BabyBear::ZERO; // should be 1

    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect_err("wrong is_full_round in preprocessed should fail");
}

// ── Invalid traces (existing, no preprocessed) ──

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

// ── T13: Poseidon round flip ──

/// T13: Swap full/partial round flags between two rows in preprocessed → should fail.
///
/// Full rounds apply the S-box to all 16 elements, partial rounds only to element 0.
/// Swapping is_full_round between a full-round row (row 0) and a partial-round row
/// (row 4, the first partial round after the initial full rounds) means the main
/// trace is computing the wrong S-box application, causing a mismatch.
#[test]
fn invalid_round_flip_full_to_partial() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    let mut prep = generate_poseidon_preprocessed(inputs.len());

    // Row 0 is a full round (is_full_round=1). Flip it to partial (0).
    // Row 4 is the first partial round (is_full_round=0). Flip it to full (1).
    let prep_width = POSEIDON_PREPROCESSED_WIDTH;

    // Flip row 0: full → partial
    {
        let cols: &mut PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[0..prep_width]);
        cols.is_full_round = BabyBear::ZERO;
    }
    // Flip row 4: partial → full
    {
        let cols: &mut PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[4 * prep_width..5 * prep_width]);
        cols.is_full_round = BabyBear::ONE;
    }

    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep)).expect_err(
        "swapping full/partial round flags must fail preprocessed RC or sbox constraint",
    );
}

/// T13b: Swap two full-round RC vectors to verify round-constant matching fails.
///
/// Even within full rounds, each row has distinct RC values. Swapping RC between
/// rounds 0 and 1 violates the main-trace RC equality constraint.
#[test]
fn invalid_round_constants_swapped_between_rounds() {
    let inputs = vec![poseidon_test_input(1)];
    let trace = generate_poseidon_trace(&inputs);
    let mut prep = generate_poseidon_preprocessed(inputs.len());

    let prep_width = POSEIDON_PREPROCESSED_WIDTH;

    // Read RC from row 0 and row 1.
    let rc0: [BabyBear; 16] = {
        let cols: &PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[0..prep_width]);
        cols.rc
    };
    let rc1: [BabyBear; 16] = {
        let cols: &PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[prep_width..2 * prep_width]);
        cols.rc
    };

    // Write rc1 into row 0 and rc0 into row 1.
    {
        let cols: &mut PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[0..prep_width]);
        cols.rc = rc1;
    }
    {
        let cols: &mut PoseidonPreprocessedCols<BabyBear> =
            borrow_cols_mut(&mut prep.values[prep_width..2 * prep_width]);
        cols.rc = rc0;
    }

    debug_check_with_preprocessed(&PoseidonChip, &trace, Some(&prep))
        .expect_err("swapped round constants must fail RC equality constraint");
}

// ── Column width tests ──

#[test]
fn width() {
    assert_eq!(POSEIDON_WIDTH, 93);
}

#[test]
fn preprocessed_width() {
    assert_eq!(POSEIDON_PREPROCESSED_WIDTH, 19);
}
