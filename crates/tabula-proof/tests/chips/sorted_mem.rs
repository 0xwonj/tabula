use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::sorted_mem::air::GlobalSortedMemChip;
use tabula_proof::air::chips::sorted_mem::columns::{
    GlobalSortedMemCols, SORTED_MEM_STANDARD_WIDTH, sorted_mem_width,
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
        (
            cols.tau.limbs.limb0,
            cols.tau.limbs.limb1,
            cols.tau.limbs.limb2,
        )
    };
    let (tau2_l0, tau2_l1, tau2_l2) = {
        let cols: &GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols(&trace.values[2 * width..3 * width]);
        (
            cols.tau.limbs.limb0,
            cols.tau.limbs.limb1,
            cols.tau.limbs.limb2,
        )
    };

    // Swap: row 1 gets tau=2, row 2 gets tau=1.
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols.tau.limbs.limb0 = tau2_l0;
        cols.tau.limbs.limb1 = tau2_l1;
        cols.tau.limbs.limb2 = tau2_l2;
    }
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[2 * width..3 * width]);
        cols.tau.limbs.limb0 = tau1_l0;
        cols.tau.limbs.limb1 = tau1_l1;
        cols.tau.limbs.limb2 = tau1_l2;
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
        cols.ordering.ineq.diff0 = BabyBear::new(999);
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

// ── Lex ordering direction tests (A2) ──

#[test]
fn soundness_reversed_table_at_boundary() {
    // Valid trace: (t=0,c=0) → (t=1,c=0). Swap table_ids to make reversed.
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        init_row(1, 0, 20, [2, 0, 0], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = sorted_mem_width::<3>();
    // Swap table_id: row 0 gets 1, row 1 gets 0
    let cols0: &mut GlobalSortedMemCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.table_id = BabyBear::ONE;
    let cols1: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.table_id = BabyBear::ZERO;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("reversed table order should fail lex or derivation constraint");
}

#[test]
fn soundness_reversed_col_same_table() {
    // Valid trace: (t=0,c=0) → (t=0,c=1). Swap col_ids to make reversed.
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        init_row(0, 1, 20, [2, 0, 0], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = sorted_mem_width::<3>();
    // Swap col_id: row 0 gets 1, row 1 gets 0
    let cols0: &mut GlobalSortedMemCols<BabyBear, 3> = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.col_id = BabyBear::ONE;
    let cols1: &mut GlobalSortedMemCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.col_id = BabyBear::ZERO;
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("reversed col order should fail lex or derivation constraint");
}

#[test]
fn valid_lex_ordering_multiple_segments() {
    // Valid: (t=0,c=0) → (t=0,c=1) → (t=1,c=0).
    let rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        init_row(0, 1, 10, [2, 0, 0], false),
        init_row(1, 0, 10, [3, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("valid lex ordering across multiple segments should pass");
}

// ── T3: Init row uniqueness ──

/// T3: Two init rows (τ=0) for the same key must fail.
///
/// The init row uniqueness constraint requires τ_{i+1} > 0 when r_{i+1} = r_i and τ_i = 0.
/// We build a valid init+read trace then forge row 1 to be a second init (τ=0, is_init=1).
/// This bypasses trace-gen's StrictIneq panic while still violating the AIR constraint.
#[test]
fn invalid_two_init_rows_same_key() {
    // Start with init(τ=0) + read(τ=1): valid trace.
    let rows = vec![
        init_row(0, 0, 100, [1, 2, 3], false),
        read_row(0, 0, 100, 1, [1, 2, 3], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;

    // Forge row 1 as a second init: set is_init=1 and timestamp=0.
    // The init format constraint requires τ=0 on init rows — this is already satisfied.
    // The init row uniqueness constraint requires τ_next > 0 when same key and τ_prev=0.
    // Row 0 has τ=0 (init), row 1 forged to τ=0 (init) → uniqueness violated.
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols.is_init = BabyBear::ONE;
        cols.tau.limbs.limb0 = BabyBear::ZERO;
        cols.tau.limbs.limb1 = BabyBear::ZERO;
        cols.tau.limbs.limb2 = BabyBear::ZERO;
        // Also set is_write=0 to match init format.
        cols.is_write = BabyBear::ZERO;
    }

    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("two init rows for same key must fail init row uniqueness constraint");
}

/// T3b: Two init rows for different keys must succeed (different key segments).
#[test]
fn valid_two_init_rows_different_keys() {
    let rows = vec![
        init_row(0, 0, 100, [1, 0, 0], false),
        init_row(0, 0, 200, [2, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("two init rows for different keys should pass");
}

// ── T5: Multiple writes same key ──

/// T5: Multiple writes to the same key at different timestamps must succeed.
///
/// The SortedMem chip must allow arbitrary writes as long as timestamps
/// strictly increase within the same key segment.
#[test]
fn valid_multiple_writes_same_key() {
    let rows = vec![
        init_row(0, 0, 100, [1, 0, 0], false),
        write_row(0, 0, 100, 1, [2, 0, 0], false),
        write_row(0, 0, 100, 2, [3, 0, 0], false),
        write_row(0, 0, 100, 3, [4, 0, 0], false),
        read_row(0, 0, 100, 4, [4, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("multiple writes to same key should pass");
}

/// T5b: Multiple writes across non-consecutive timestamps must succeed.
#[test]
fn valid_multiple_writes_sparse_timestamps() {
    let rows = vec![
        init_row(0, 0, 50, [10, 0, 0], false),
        write_row(0, 0, 50, 5, [20, 0, 0], false),
        write_row(0, 0, 50, 100, [30, 0, 0], false),
        read_row(0, 0, 50, 200, [30, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("sparse timestamp writes should pass");
}

// ── T12: Large tau ──

/// T12: Large timestamp values (near 2^60) must be handled correctly.
#[test]
fn valid_large_tau() {
    let large_tau = (1u64 << 60) + 42;
    let rows = vec![
        init_row(0, 0, 100, [1, 0, 0], false),
        read_row(0, 0, 100, large_tau, [1, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace).expect("large tau should pass");
}

/// T12b: Large tau with write and subsequent read must succeed.
#[test]
fn valid_large_tau_write_then_read() {
    let tau_write = (1u64 << 50) + 7;
    let tau_read = (1u64 << 50) + 100;
    let rows = vec![
        init_row(0, 0, 200, [5, 0, 0], false),
        write_row(0, 0, 200, tau_write, [99, 0, 0], false),
        read_row(0, 0, 200, tau_read, [99, 0, 0], false),
    ];
    let trace = generate_sorted_mem_trace::<3>(&rows);
    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect("large tau with write+read should pass");
}

/// T12c: Tau not strictly increasing within same key segment must fail.
///
/// The ordering constraint requires tau strictly increases for same (t,c,r).
/// Build a valid trace (tau=5 then tau=10), then tamper tau of row 2 to equal tau of row 1.
/// We avoid calling trace gen with equal taus (which panics in StrictIneq).
#[test]
fn invalid_tau_not_strictly_increasing() {
    // Build valid trace: init(τ=0) + read(τ=5) + read(τ=10).
    let rows = vec![
        init_row(0, 0, 100, [1, 0, 0], false),
        read_row(0, 0, 100, 5, [1, 0, 0], false),
        read_row(0, 0, 100, 10, [1, 0, 0], false),
    ];
    let mut trace = generate_sorted_mem_trace::<3>(&rows);
    let width = SORTED_MEM_STANDARD_WIDTH;

    // Tamper row 2 tau from 10 → 5 (equal to row 1's tau → not strictly increasing).
    {
        let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
            borrow_cols_mut(&mut trace.values[2 * width..3 * width]);
        cols.tau.limbs.limb0 = BabyBear::new(5);
        cols.tau.limbs.limb1 = BabyBear::ZERO;
        cols.tau.limbs.limb2 = BabyBear::ZERO;
    }

    debug_check(&GlobalSortedMemChip::<3>, &trace)
        .expect_err("equal tau for same key should fail strict monotone constraint");
}

// ── Column width test ──

#[test]
fn standard_width() {
    assert_eq!(SORTED_MEM_STANDARD_WIDTH, 67);
}
