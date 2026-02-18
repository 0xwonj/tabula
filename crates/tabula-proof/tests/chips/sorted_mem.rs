use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::sorted_mem::air::GlobalSortedMemChip;
use tabula_proof::air::chips::sorted_mem::columns::{
    GlobalSortedMemCols, SORTED_MEM_STANDARD_WIDTH,
};
use tabula_proof::air::chips::sorted_mem::trace::{SortedMemRow, generate_sorted_mem_trace};
use tabula_proof::air::{borrow_cols, borrow_cols_mut, debug_check};

use crate::common::builders::{init_row, read_row, write_row};

// ── Valid traces ──

#[test]
fn valid_single_init_only() {
    let rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("single init should pass");
}

#[test]
fn valid_init_then_read() {
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("init+read should pass");
}

#[test]
fn valid_init_read_write_read() {
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
        write_row(0, 0, 100, 2, [4, 5, 6], false),
        read_row(0, 0, 100, 3, [4, 5, 6], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("init+read+write+read should pass");
}

#[test]
fn valid_two_keys_same_column() {
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        read_row(0, 0, 10, 1, [1, 0, 0], false),
        init_row(0, 0, 20, [2, 0, 0], false),
        read_row(0, 0, 20, 1, [2, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("two keys same col should pass");
}

#[test]
fn valid_null_init_and_write() {
    let rows = vec![
        init_row(0, 0, 100, [0, 0, 0], true),
        write_row(0, 0, 100, 1, [1, 2, 3], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("null init + write should pass");
}

#[test]
fn valid_two_segments_different_tc() {
    let rows = vec![
        init_row(0, 0, 100, [1, 0, 0], false),
        read_row(0, 0, 100, 1, [1, 0, 0], false),
        init_row(0, 1, 10, [2, 0, 0], false),
        read_row(0, 1, 10, 1, [2, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("cross-(t,c) with decreasing r should pass");
}

#[test]
fn valid_all_padding() {
    let rows: Vec<SortedMemRow> = vec![];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("all-padding should pass");
}

#[test]
fn valid_write_set_extraction() {
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
        write_row(0, 0, 100, 2, [4, 5, 6], false),
        read_row(0, 0, 100, 3, [4, 5, 6], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("write-set should pass");
}

// ── Invalid traces ──

#[test]
fn invalid_missing_init() {
    let mut rows = vec![read_row(0, 0, 100, 1, [1, 2, 3], false)];
    rows[0].is_init = false;
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("missing init should fail (first row must be init)");
}

#[test]
fn invalid_init_with_nonzero_tau() {
    let mut rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
    rows[0].timestamp = 5;
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect_err("init with nonzero tau should fail");
}

#[test]
fn invalid_init_with_write() {
    let mut rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
    rows[0].is_write = true;
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect_err("init with is_write=1 should fail");
}

#[test]
fn invalid_read_wrong_value() {
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        SortedMemRow {
            table_id: 0,
            col_id: 0,
            row_key: 100,
            timestamp: 1,
            is_init: false,
            is_write: false,
            val: vec![BabyBear::new(999), BabyBear::new(2), BabyBear::new(3)],
            val_is_null: false,
            meta_is_empty_old: false,
        },
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect_err("read with wrong value should fail");
}

#[test]
fn invalid_null_canon_violation() {
    let rows = vec![init_row(0, 0, 100, [1, 2, 3], true)];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect_err("null canon violation should fail");
}

// ── Ordering violation tests ──

#[test]
fn invalid_tau_regression() {
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
        read_row(0, 0, 100, 2, [1, 2, 3], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;

    // Save tau from row 1 and row 2.
    let (tau1_l0, tau1_l1, tau1_l2) = {
        let cols: &GlobalSortedMemCols<BabyBear, 3> = borrow_cols(&trace.values[width..2 * width]);
        (cols.tau.limb0, cols.tau.limb1, cols.tau.limb2)
    };
    let (tau2_l0, tau2_l1, tau2_l2) = {
        let cols: &GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols(&trace.values[2 * width..3 * width]);
        (cols.tau.limb0, cols.tau.limb1, cols.tau.limb2)
    };

    // Swap: row 1 gets tau=2, row 2 gets tau=1.
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols.tau.limb0 = tau2_l0;
        cols.tau.limb1 = tau2_l1;
        cols.tau.limb2 = tau2_l2;
    }
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[2 * width..3 * width]);
        cols.tau.limb0 = tau1_l0;
        cols.tau.limb1 = tau1_l1;
        cols.tau.limb2 = tau1_l2;
    }

    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("tau regression (2 → 1) should fail ordering constraint");
}

#[test]
fn invalid_ordering_witness_corrupted() {
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        init_row(0, 0, 20, [2, 0, 0], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;

    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[0..width]);
        cols.ordering.diff0 = BabyBear::new(999);
    }

    debug_check(&GlobalSortedMemChip::<3>, &trace).expect_err("corrupted ordering gap should fail");
}

// ── Soundness tests ──

#[test]
fn soundness_is_real_prefix_gap() {
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        init_row(0, 0, 20, [2, 0, 0], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;
    // Set row 0 is_real=0, keep row 1 is_real=1 → 0→1 violates prefix
    let cols: &mut GlobalSortedMemCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_real = BabyBear::ZERO;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("is_real 0→1 should fail prefix constraint");
}

#[test]
fn soundness_forged_init_mid_segment() {
    // init(tau=0) → read(tau=1), then forge read row to be init
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;
    // Set row 1 is_init=1 (forged). Init format requires tau=0, but tau=1 → fail.
    let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.is_init = BabyBear::ONE;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("forged init mid-segment should fail (tau≠0 on init)");
}

#[test]
fn soundness_has_written_forged_zero() {
    // init → write → read. Forge write row's has_written to 0.
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        write_row(0, 0, 100, 1, [4, 5, 6], false),
        read_row(0, 0, 100, 2, [4, 5, 6], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;
    // Forge write row (row 1): has_written = 0 (should be 1)
    let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.has_written = BabyBear::ZERO;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("forged has_written=0 after write should fail propagation");
}

#[test]
fn soundness_write_mem_not_updated() {
    // init → write. Corrupt write row's mem to differ from its val.
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        write_row(0, 0, 100, 1, [4, 5, 6], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;
    // Corrupt write row (row 1): mem[0] ≠ val[0]
    let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.mem[0] = BabyBear::new(999);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("write with corrupted mem should fail mem transition");
}

#[test]
fn soundness_is_last_for_key_forged() {
    // init → read. Forge is_last_for_key=0 on the last real row.
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;
    // Row 1 (last real) should have is_last_for_key=1 and r_changed=1.
    // Forge is_last_for_key=0. The real→padding constraint requires it to be 1.
    let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.is_last_for_key = BabyBear::ZERO;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("forged is_last_for_key=0 on last real row should fail");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(SORTED_MEM_STANDARD_WIDTH, 42);
}
