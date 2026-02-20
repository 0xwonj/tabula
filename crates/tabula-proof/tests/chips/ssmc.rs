use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::ssmc::air::GlobalSsmcChip;
use tabula_proof::air::chips::ssmc::columns::{SSMC_STANDARD_WIDTH, ssmc_width};
use tabula_proof::air::chips::ssmc::trace::{SsmcEntry, generate_ssmc_trace};
use tabula_proof::air::{borrow_cols, borrow_cols_mut, debug_check};

use crate::common::builders::ssmc_entry;

type SsmcCols = tabula_proof::air::chips::ssmc::columns::GlobalSsmcCols<BabyBear, 3>;

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
    let width = ssmc_width::<3>();
    let cols: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_first = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_first should fail");
}

#[test]
fn invalid_is_last_wrong() {
    let entries = vec![ssmc_entry(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    let cols: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_last = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_last should fail");
}

#[test]
fn invalid_boundary_flags_mismatch() {
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    let cols: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_last = BabyBear::ONE;
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
        let cols0: &SsmcCols = borrow_cols(&trace.values[0..width]);
        let cols1: &SsmcCols = borrow_cols(&trace.values[width..2 * width]);
        (
            (
                cols0.key.limbs.limb0,
                cols0.key.limbs.limb1,
                cols0.key.limbs.limb2,
            ),
            (
                cols1.key.limbs.limb0,
                cols1.key.limbs.limb1,
                cols1.key.limbs.limb2,
            ),
        )
    };
    {
        let cols0: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
        cols0.key.limbs.limb0 = key1.0;
        cols0.key.limbs.limb1 = key1.1;
        cols0.key.limbs.limb2 = key1.2;
    }
    {
        let cols1: &mut SsmcCols = borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols1.key.limbs.limb0 = key0.0;
        cols1.key.limbs.limb1 = key0.1;
        cols1.key.limbs.limb2 = key0.2;
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
        let cols0: &SsmcCols = borrow_cols(&trace.values[0..width]);
        (
            cols0.key.limbs.limb0,
            cols0.key.limbs.limb1,
            cols0.key.limbs.limb2,
        )
    };
    let cols1: &mut SsmcCols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.key.limbs.limb0 = key0.0;
    cols1.key.limbs.limb1 = key0.1;
    cols1.key.limbs.limb2 = key0.2;
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
    let cols: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.segment.tc_changed = BabyBear::ONE;
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("forged tc_changed should fail derivation constraint");
}

// ── Lex ordering direction tests (A2) ──

#[test]
fn soundness_reversed_table_at_boundary() {
    // Valid trace: (t=0,c=0) → (t=1,c=0). Swap table_ids to make (t=1) → (t=0).
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(1, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    // Swap table_id: row 0 gets 1, row 1 gets 0
    let cols0: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.table_id = BabyBear::ONE;
    let cols1: &mut SsmcCols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.table_id = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("reversed table order at boundary should fail lex or derivation constraint");
}

#[test]
fn soundness_reversed_col_same_table() {
    // Valid trace: (t=0,c=0) → (t=0,c=1). Swap col_ids to make c=1 → c=0.
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 1, 20, [2, 0, 0]),
    ];
    let mut trace = generate_ssmc_trace::<3>(&entries);
    let width = ssmc_width::<3>();
    // Swap col_id: row 0 gets 1, row 1 gets 0
    let cols0: &mut SsmcCols = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.col_id = BabyBear::ONE;
    let cols1: &mut SsmcCols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.col_id = BabyBear::ZERO;
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect_err("reversed col order at boundary should fail lex or derivation constraint");
}

#[test]
fn valid_lex_ordering_multiple_segments() {
    // Valid: (t=0,c=0) → (t=0,c=1) → (t=1,c=0) — correct lex order.
    let entries = vec![
        ssmc_entry(0, 0, 10, [1, 0, 0]),
        ssmc_entry(0, 1, 10, [2, 0, 0]),
        ssmc_entry(1, 0, 10, [3, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect("valid lex ordering across multiple segments should pass");
}

// ── T11: SSMC u64::MAX key ──

/// T11: Entry with key=u64::MAX must be handled correctly.
///
/// u64::MAX = (2^30-1) + (2^30-1)*2^30 + 15*2^60 in our 30+30+4 limb encoding.
/// The StrictIneq ordering constraint for a single-entry segment (is_last=1, is_first=1)
/// doesn't require gap from the next key, so this should pass.
#[test]
fn valid_key_u64_max() {
    let entries = vec![ssmc_entry(0, 0, u64::MAX, [7, 0, 0])];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace).expect("key=u64::MAX should pass");
}

/// T11b: Two entries where second key = u64::MAX — largest possible key gap.
#[test]
fn valid_two_entries_ending_at_u64_max() {
    let entries = vec![
        ssmc_entry(0, 0, 1, [1, 0, 0]),
        ssmc_entry(0, 0, u64::MAX, [2, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect("entries [1, u64::MAX] should pass (valid gap)");
}

/// T11c: u64::MAX key in multiple segments (each segment has its own key space).
#[test]
fn valid_u64_max_key_different_segments() {
    let entries = vec![
        ssmc_entry(0, 0, u64::MAX, [1, 0, 0]),
        ssmc_entry(0, 1, u64::MAX, [2, 0, 0]),
        ssmc_entry(1, 0, u64::MAX, [3, 0, 0]),
    ];
    let trace = generate_ssmc_trace::<3>(&entries);
    debug_check(&GlobalSsmcChip::<3>, &trace)
        .expect("u64::MAX key in different segments should pass");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(SSMC_STANDARD_WIDTH, 66);
}
