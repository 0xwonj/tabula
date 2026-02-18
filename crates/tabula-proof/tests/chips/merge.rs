use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::merge::air::GlobalMergeChip;
use tabula_proof::air::chips::merge::columns::{
    GlobalMergeCols, MERGE_STANDARD_WIDTH, merge_width,
};
use tabula_proof::air::chips::merge::trace::{MergeRow, MergeSource, generate_merge_trace};
use tabula_proof::air::{borrow_cols, borrow_cols_mut, debug_check};

use crate::common::builders::{
    both_row, delete_row, merge_val, merge_zeros, old_only_row, write_only_row,
};

// ── Valid traces ──

#[test]
fn valid_old_only() {
    let rows = vec![old_only_row(0, 0, 10, [1, 2, 3])];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("old_only should pass");
}

#[test]
fn valid_write_only() {
    let rows = vec![write_only_row(0, 0, 10, [4, 5, 6])];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("write_only should pass");
}

#[test]
fn valid_both() {
    let rows = vec![both_row(0, 0, 10, [1, 2, 3], [4, 5, 6])];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("both should pass");
}

#[test]
fn valid_delete() {
    let rows = vec![delete_row(0, 0, 10, [1, 2, 3])];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("delete should pass");
}

#[test]
fn valid_mixed_sources() {
    let rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        both_row(0, 0, 20, [2, 0, 0], [7, 0, 0]),
        delete_row(0, 0, 30, [3, 0, 0]),
        write_only_row(0, 0, 40, [8, 0, 0]),
    ];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("mixed sources should pass");
}

#[test]
fn valid_multi_segment() {
    let rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        write_only_row(0, 1, 5, [2, 0, 0]),
    ];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("multi-segment should pass");
}

#[test]
fn valid_all_deletes() {
    let rows = vec![
        delete_row(0, 0, 10, [1, 0, 0]),
        delete_row(0, 0, 20, [2, 0, 0]),
    ];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("all deletes should pass");
}

#[test]
fn valid_all_padding() {
    let rows: Vec<MergeRow> = vec![];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("all-padding should pass");
}

#[test]
fn valid_hash_acc_carry_through_delete() {
    let hash = [BabyBear::new(42); 8];
    let rows = vec![
        MergeRow {
            hash_acc: hash,
            ..old_only_row(0, 0, 10, [1, 0, 0])
        },
        MergeRow {
            hash_acc: hash,
            ..delete_row(0, 0, 20, [2, 0, 0])
        },
        MergeRow {
            hash_acc: hash,
            ..old_only_row(0, 0, 30, [3, 0, 0])
        },
    ];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect("hash_acc carry through delete should pass");
}

// ── Invalid traces ──

#[test]
fn invalid_old_only_wrong_new_val() {
    let mut rows = vec![old_only_row(0, 0, 10, [1, 2, 3])];
    rows[0].new_val = merge_val([9, 9, 9]);
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect_err("wrong new_val for old_only should fail");
}

#[test]
fn invalid_write_only_wrong_new_val() {
    let mut rows = vec![write_only_row(0, 0, 10, [4, 5, 6])];
    rows[0].new_val = merge_val([1, 1, 1]);
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("wrong new_val for write_only should fail");
}

#[test]
fn invalid_both_wrong_new_val() {
    let mut rows = vec![both_row(0, 0, 10, [1, 2, 3], [4, 5, 6])];
    rows[0].new_val = merge_val([1, 2, 3]);
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect_err("wrong new_val for both should fail");
}

#[test]
fn invalid_delete_in_new_one() {
    let mut rows = vec![delete_row(0, 0, 10, [1, 2, 3])];
    rows[0].in_new = true;
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace).expect_err("delete with in_new=1 should fail");
}

#[test]
fn invalid_hash_acc_changed_through_delete() {
    let hash1 = [BabyBear::new(42); 8];
    let hash2 = [BabyBear::new(99); 8];
    let rows = vec![
        MergeRow {
            hash_acc: hash1,
            ..old_only_row(0, 0, 10, [1, 0, 0])
        },
        MergeRow {
            hash_acc: hash1,
            ..delete_row(0, 0, 20, [2, 0, 0])
        },
        MergeRow {
            hash_acc: hash2,
            ..old_only_row(0, 0, 30, [3, 0, 0])
        },
    ];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("hash_acc changed through delete should fail");
}

#[test]
fn invalid_delete_nonzero_write_val() {
    let rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 10,
        source: MergeSource::Delete,
        old_val: merge_val([1, 0, 0]),
        write_val: merge_val([9, 0, 0]),
        new_val: merge_zeros(),
        in_new: false,
        hash_acc: [BabyBear::ZERO; 8],
    }];
    let trace = generate_merge_trace::<3>(&rows);
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("delete with nonzero write_val should fail");
}

// ── Soundness tests ──

#[test]
fn soundness_key_order_swapped() {
    // Keys 10, 20 in same segment. Swap to 20, 10.
    let rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        old_only_row(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_merge_trace::<3>(&rows);
    let width = merge_width::<3>();
    // Swap key limbs between row 0 and row 1
    let (key0, key1) = {
        let cols0: &GlobalMergeCols<BabyBear, 3> = borrow_cols(&trace.values[0..width]);
        let cols1: &GlobalMergeCols<BabyBear, 3> = borrow_cols(&trace.values[width..2 * width]);
        (
            (cols0.key.limb0, cols0.key.limb1, cols0.key.limb2),
            (cols1.key.limb0, cols1.key.limb1, cols1.key.limb2),
        )
    };
    {
        let cols0: &mut GlobalMergeCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
        cols0.key.limb0 = key1.0;
        cols0.key.limb1 = key1.1;
        cols0.key.limb2 = key1.2;
    }
    {
        let cols1: &mut GlobalMergeCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols1.key.limb0 = key0.0;
        cols1.key.limb1 = key0.1;
        cols1.key.limb2 = key0.2;
    }
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("swapped key order should fail StrictIneq constraint");
}

#[test]
fn soundness_is_real_prefix_gap() {
    let rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        old_only_row(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_merge_trace::<3>(&rows);
    let width = merge_width::<3>();
    // Set row 0 is_real=0, keep row 1 is_real=1 → 0→1 violates prefix
    let cols: &mut GlobalMergeCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_real = BabyBear::ZERO;
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("is_real 0→1 should fail prefix constraint");
}

#[test]
fn soundness_tc_changed_forged() {
    // Two rows in same segment. Forge tc_changed=1 on row 0.
    let rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        old_only_row(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_merge_trace::<3>(&rows);
    let width = merge_width::<3>();
    let cols: &mut GlobalMergeCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.tc_changed = BabyBear::ONE;
    debug_check(&GlobalMergeChip::<3>, &trace)
        .expect_err("forged tc_changed should fail derivation constraint");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(MERGE_STANDARD_WIDTH, 34);
}
