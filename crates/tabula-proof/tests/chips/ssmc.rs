use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::ssmc::air::GlobalSsmcChip;
use tabula_proof::air::chips::ssmc::columns::{GlobalSsmcCols, SSMC_STANDARD_WIDTH, ssmc_width};
use tabula_proof::air::chips::ssmc::trace::{SsmcEntry, generate_ssmc_trace};
use tabula_proof::air::{borrow_cols, borrow_cols_mut, debug_check};

use crate::common::builders::ssmc_entry;

// ── Valid traces ──

#[test]
fn valid_single_entry() {
    let entries = vec![ssmc_entry(0, 0, 100, [1, 2, 3])];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("single entry should pass");
}

#[test]
fn valid_multiple_entries_same_segment() {
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
        ssmc_entry(0, 0, 30, [3, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("multi-entry same segment should pass");
}

#[test]
fn valid_two_segments() {
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
        ssmc_entry(0, 1, 5, [3, 0, 0]),
        ssmc_entry(0, 1, 15, [4, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("two segments should pass");
}

#[test]
fn valid_different_tables() {
    let entries = vec![
        ssmc_entry(0, 0, 100, [1, 0, 0]),
        ssmc_entry(1, 0, 50, [2, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("different tables should pass");
}

#[test]
fn valid_all_padding() {
    let entries: Vec<SsmcEntry> = vec![];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("all-padding should pass");
}

#[test]
fn valid_large_keys() {
    let entries = vec![
        ssmc_entry(0, 0, 1 << 30, [1, 0, 0]),
        ssmc_entry(0, 0, (1 << 60) + 1, [2, 0, 0]),
        ssmc_entry(0, 0, u64::MAX - 1, [3, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("large keys should pass");
}

#[test]
fn valid_single_entry_per_segment() {
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 1, 20, [2, 0, 0]),
        ssmc_entry(1, 0, 30, [3, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("single entry per segment should pass");
}

// ── Invalid traces ──

#[test]
fn invalid_broken_is_real_prefix() {
    let entries = vec![ssmc_entry(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    trace.values[0] = BabyBear::ZERO; // row 0: is_real = 0
    trace.values[width] = BabyBear::ONE; // row 1: is_real = 1
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("broken is_real prefix should fail");
}

#[test]
fn invalid_is_first_wrong() {
    let entries = vec![ssmc_entry(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let is_first_offset = 9;
    trace.values[is_first_offset] = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_first should fail");
}

#[test]
fn invalid_is_last_wrong() {
    let entries = vec![ssmc_entry(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let is_last_offset = 10;
    trace.values[is_last_offset] = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_last should fail");
}

#[test]
fn invalid_boundary_flags_mismatch() {
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let is_last_offset = 10;
    trace.values[is_last_offset] = BabyBear::ONE;
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong boundary flag should fail");
}

// ── Soundness tests ──

#[test]
fn soundness_key_order_swapped() {
    // Keys 10, 20 in same segment. Swap key limbs to make 20, 10.
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    // Swap key values between row 0 and row 1
    let (key0, key1) = {
        let cols0: &GlobalSsmcCols<BabyBear, 3> = borrow_cols(&trace.values[0..width]);
        let cols1: &GlobalSsmcCols<BabyBear, 3> = borrow_cols(&trace.values[width..2 * width]);
        (
            (cols0.key.limb0, cols0.key.limb1, cols0.key.limb2),
            (cols1.key.limb0, cols1.key.limb1, cols1.key.limb2),
        )
    };
    {
        let cols0: &mut GlobalSsmcCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
        cols0.key.limb0 = key1.0;
        cols0.key.limb1 = key1.1;
        cols0.key.limb2 = key1.2;
    }
    {
        let cols1: &mut GlobalSsmcCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols1.key.limb0 = key0.0;
        cols1.key.limb1 = key0.1;
        cols1.key.limb2 = key0.2;
    }
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("swapped key order should fail StrictIneq constraint");
}

#[test]
fn soundness_duplicate_key_in_segment() {
    // Keys 10, 20: change row 1's key to 10 (duplicate).
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    // Copy row 0's key to row 1
    let key0 = {
        let cols0: &GlobalSsmcCols<BabyBear, 3> = borrow_cols(&trace.values[0..width]);
        (cols0.key.limb0, cols0.key.limb1, cols0.key.limb2)
    };
    let cols1: &mut GlobalSsmcCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.key.limb0 = key0.0;
    cols1.key.limb1 = key0.1;
    cols1.key.limb2 = key0.2;
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("duplicate key should fail strict inequality");
}

#[test]
fn soundness_tc_changed_forged() {
    // Two entries in same segment. Forge tc_changed=1 on row 0.
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    let cols: &mut GlobalSsmcCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.tc_changed = BabyBear::ONE;
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("forged tc_changed should fail derivation constraint");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(SSMC_STANDARD_WIDTH, 27);
}
