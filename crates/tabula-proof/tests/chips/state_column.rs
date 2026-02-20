use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::state_column::air::StateColumnChip;
use tabula_proof::air::chips::state_column::columns::{
    STATE_COLUMN_STANDARD_WIDTH, state_column_width,
};
use tabula_proof::air::chips::state_column::trace::{
    EntrySource, StateColumnRow, generate_state_column_trace,
};
use tabula_proof::air::{StateColumnCols, borrow_cols, borrow_cols_mut, debug_check};

use crate::common::builders::{sc_both, sc_delete, sc_gap, sc_old_only, sc_write_only};

type Cols = StateColumnCols<BabyBear, 3>;

// ── Column width ──

#[test]
fn standard_width() {
    assert_eq!(STATE_COLUMN_STANDARD_WIDTH, 100);
}

// ── Valid traces: single source types ──

#[test]
fn valid_single_old_only() {
    let rows = vec![sc_old_only(0, 0, 100, [1, 2, 3])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single old_only should pass");
}

#[test]
fn valid_single_write_only() {
    let rows = vec![sc_write_only(0, 0, 100, [4, 5, 6])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single write_only should pass");
}

#[test]
fn valid_single_both() {
    let rows = vec![sc_both(0, 0, 100, [1, 2, 3], [4, 5, 6])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single both should pass");
}

#[test]
fn valid_single_delete() {
    let rows = vec![sc_delete(0, 0, 100, [1, 2, 3])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single delete should pass");
}

#[test]
fn valid_single_gap() {
    let rows = vec![sc_gap(0, 0, 100)];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single gap should pass");
}

// ── Valid traces: mixed sources ──

#[test]
fn valid_mixed_sources_same_segment() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_both(0, 0, 20, [2, 0, 0], [7, 0, 0]),
        sc_delete(0, 0, 30, [3, 0, 0]),
        sc_write_only(0, 0, 40, [8, 0, 0]),
    ];
    // Mark all as touched since writes exist
    let rows: Vec<_> = rows
        .into_iter()
        .map(|mut r| {
            r.segment_is_touched = true;
            r
        })
        .collect();
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("mixed sources should pass");
}

// ── Valid traces: gap rows ──

#[test]
fn valid_gap_between_entries() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_gap(0, 0, 15),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("gap between entries should pass");
}

#[test]
fn valid_gap_before_first_entry() {
    let rows = vec![sc_gap(0, 0, 5), sc_old_only(0, 0, 10, [1, 0, 0])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("gap before first entry should pass");
}

#[test]
fn valid_gap_after_last_entry() {
    let rows = vec![sc_old_only(0, 0, 10, [1, 0, 0]), sc_gap(0, 0, 15)];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("gap after last entry should pass");
}

#[test]
fn valid_all_gaps() {
    let rows = vec![sc_gap(0, 0, 10), sc_gap(0, 0, 20), sc_gap(0, 0, 30)];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("all gaps (no entries) should pass");
}

// ── Valid traces: multi-segment ──

#[test]
fn valid_two_segments() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
        sc_old_only(0, 1, 5, [3, 0, 0]),
        sc_old_only(0, 1, 15, [4, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("two segments should pass");
}

#[test]
fn valid_different_tables() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(1, 0, 50, [2, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("different tables should pass");
}

#[test]
fn valid_lex_ordering_multiple_segments() {
    // (t=0,c=0) → (t=0,c=1) → (t=1,c=0)
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 1, 10, [2, 0, 0]),
        sc_old_only(1, 0, 10, [3, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("lex ordering should pass");
}

// ── Valid traces: untouched column ──

#[test]
fn valid_untouched_column_all_old_only() {
    // All old_only, segment_is_touched=false (no writes)
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
        sc_old_only(0, 0, 30, [3, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("untouched column should pass");
}

// ── Valid traces: large keys ──

#[test]
fn valid_large_keys() {
    let rows = vec![
        sc_old_only(0, 0, 1 << 30, [1, 0, 0]),
        sc_old_only(0, 0, (1 << 60) + 1, [2, 0, 0]),
        sc_old_only(0, 0, u64::MAX - 1, [3, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("large keys should pass");
}

#[test]
fn valid_key_u64_max() {
    let rows = vec![sc_old_only(0, 0, u64::MAX, [7, 0, 0])];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("key=u64::MAX should pass");
}

// ── Valid traces: all padding ──

#[test]
fn valid_all_padding() {
    let rows: Vec<StateColumnRow> = vec![];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("all-padding should pass");
}

// ── Valid traces: touched segment with mixed write types ──

#[test]
fn valid_touched_segment_write_only_and_delete() {
    let mut rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_write_only(0, 0, 15, [5, 0, 0]),
        sc_delete(0, 0, 20, [2, 0, 0]),
        sc_old_only(0, 0, 30, [3, 0, 0]),
    ];
    for r in &mut rows {
        r.segment_is_touched = true;
    }
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect("touched segment with writes + deletes should pass");
}

// ── Valid traces: gap + entry interleaving ──

#[test]
fn valid_gap_entry_gap_entry() {
    let rows = vec![
        sc_gap(0, 0, 5),
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_gap(0, 0, 15),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("gap-entry interleaving should pass");
}

// ── Invalid traces ──

#[test]
fn invalid_broken_is_real_prefix() {
    let rows = vec![sc_old_only(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    trace.values[0] = BabyBear::ZERO; // row 0: is_real = 0
    trace.values[width] = BabyBear::ONE; // row 1: is_real = 1
    debug_check(&StateColumnChip::<3>, &trace).expect_err("broken is_real prefix should fail");
}

#[test]
fn invalid_gap_with_nonzero_s1() {
    let rows = vec![sc_gap(0, 0, 10)];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.s1 = BabyBear::ONE;
    debug_check(&StateColumnChip::<3>, &trace).expect_err("gap with s1=1 should fail");
}

#[test]
fn invalid_gap_with_nonzero_old_val() {
    let rows = vec![sc_gap(0, 0, 10)];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.old_val[0] = BabyBear::new(42);
    debug_check(&StateColumnChip::<3>, &trace).expect_err("gap with nonzero old_val should fail");
}

#[test]
fn invalid_old_only_wrong_new_val() {
    let mut rows = vec![sc_old_only(0, 0, 10, [1, 2, 3])];
    rows[0].new_val = vec![BabyBear::new(9), BabyBear::new(9), BabyBear::new(9)];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("old_only with new_val != old_val should fail");
}

#[test]
fn invalid_write_only_nonzero_old_val() {
    let mut rows = vec![sc_write_only(0, 0, 10, [4, 5, 6])];
    rows[0].old_val = vec![BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("write_only with nonzero old_val should fail");
}

#[test]
fn invalid_delete_nonzero_new_val() {
    let mut rows = vec![sc_delete(0, 0, 10, [1, 2, 3])];
    rows[0].new_val = vec![BabyBear::new(9), BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("delete with nonzero new_val should fail");
}

#[test]
fn invalid_segment_is_touched_inconsistent() {
    // Two rows in same segment with different segment_is_touched
    let rows = vec![
        StateColumnRow {
            segment_is_touched: true,
            ..sc_old_only(0, 0, 10, [1, 0, 0])
        },
        StateColumnRow {
            segment_is_touched: false,
            ..sc_old_only(0, 0, 20, [2, 0, 0])
        },
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("inconsistent segment_is_touched should fail");
}

// ── Soundness tests ──

#[test]
fn soundness_key_order_swapped() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    // Swap key limbs between row 0 and row 1
    let (key0, key1) = {
        let cols0: &Cols = borrow_cols(&trace.values[0..width]);
        let cols1: &Cols = borrow_cols(&trace.values[width..2 * width]);
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
        let cols0: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
        cols0.key.limbs.limb0 = key1.0;
        cols0.key.limbs.limb1 = key1.1;
        cols0.key.limbs.limb2 = key1.2;
    }
    {
        let cols1: &mut Cols = borrow_cols_mut(&mut trace.values[width..2 * width]);
        cols1.key.limbs.limb0 = key0.0;
        cols1.key.limbs.limb1 = key0.1;
        cols1.key.limbs.limb2 = key0.2;
    }
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("swapped key order should fail StrictIneq constraint");
}

#[test]
fn soundness_tc_changed_forged() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.segment.tc_changed = BabyBear::ONE;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("forged tc_changed should fail derivation constraint");
}

#[test]
fn soundness_reversed_table_at_boundary() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(1, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    // Swap table_ids: row 0 gets 1, row 1 gets 0
    let cols0: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.table_id = BabyBear::ONE;
    let cols1: &mut Cols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.table_id = BabyBear::ZERO;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("reversed table order should fail lex or derivation constraint");
}

#[test]
fn soundness_reversed_col_same_table() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 1, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    // Swap col_ids
    let cols0: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols0.col_id = BabyBear::ONE;
    let cols1: &mut Cols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.col_id = BabyBear::ZERO;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("reversed col order should fail lex or derivation constraint");
}

#[test]
fn soundness_is_real_prefix_gap_in_middle() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    // Set row 0 is_real=0, keep row 1 is_real=1
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_real = BabyBear::ZERO;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("is_real 0→1 should fail prefix constraint");
}

// ── Chain tracking tests ──

#[test]
fn invalid_is_last_old_on_gap() {
    // Gap row should not have is_last_old_entry=1
    let rows = vec![sc_gap(0, 0, 10)];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_last_old_entry = BabyBear::ONE;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("is_last_old on gap should fail (is_last_old implies in_old)");
}

#[test]
fn invalid_is_last_old_on_write_only() {
    // write_only has in_old=0, so is_last_old_entry=1 should fail
    let rows = vec![sc_write_only(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_last_old_entry = BabyBear::ONE;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("is_last_old on write_only should fail (write_only not in_old)");
}

#[test]
fn invalid_is_last_new_on_delete() {
    // delete has in_new=0, so is_last_new_entry=1 should fail
    let rows = vec![sc_delete(0, 0, 10, [1, 0, 0])];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    let cols: &mut Cols = borrow_cols_mut(&mut trace.values[0..width]);
    cols.is_last_new_entry = BabyBear::ONE;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("is_last_new on delete should fail (delete not in_new)");
}

// ── Hash chain carry tests ──

#[test]
fn valid_old_hash_acc_carry_through_gap() {
    // Gap between two old entries: old_hash_acc carries unchanged.
    let hash = [BabyBear::new(42); 8];
    let rows = vec![
        StateColumnRow {
            old_hash_acc: hash,
            ..sc_old_only(0, 0, 10, [1, 0, 0])
        },
        StateColumnRow {
            old_hash_acc: hash,
            ..sc_gap(0, 0, 15)
        },
        StateColumnRow {
            old_hash_acc: hash,
            ..sc_old_only(0, 0, 20, [2, 0, 0])
        },
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("old_hash_acc carry through gap should pass");
}

#[test]
fn invalid_old_hash_acc_changed_through_gap() {
    // Gap between entries with hash_acc changed — should fail carry constraint
    let hash1 = [BabyBear::new(42); 8];
    let hash2 = [BabyBear::new(99); 8];
    let rows = vec![
        StateColumnRow {
            old_hash_acc: hash1,
            ..sc_old_only(0, 0, 10, [1, 0, 0])
        },
        StateColumnRow {
            old_hash_acc: hash2, // changed!
            ..sc_gap(0, 0, 15)
        },
        StateColumnRow {
            old_hash_acc: hash2,
            ..sc_old_only(0, 0, 20, [2, 0, 0])
        },
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("old_hash_acc changed through gap should fail carry constraint");
}

#[test]
fn valid_new_hash_acc_carry_through_delete() {
    // Delete entry (in_new=0) between two in_new entries: new_hash_acc carries.
    let hash = [BabyBear::new(55); 8];
    let mut rows = vec![
        StateColumnRow {
            new_hash_acc: hash,
            ..sc_old_only(0, 0, 10, [1, 0, 0])
        },
        StateColumnRow {
            new_hash_acc: hash,
            ..sc_delete(0, 0, 20, [2, 0, 0])
        },
        StateColumnRow {
            new_hash_acc: hash,
            ..sc_old_only(0, 0, 30, [3, 0, 0])
        },
    ];
    for r in &mut rows {
        r.segment_is_touched = true;
    }
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect("new_hash_acc carry through delete should pass");
}

// ── Entry source tests ──

#[test]
fn entry_source_in_old() {
    assert!(EntrySource::OldOnly.in_old());
    assert!(!EntrySource::WriteOnly.in_old());
    assert!(EntrySource::Both.in_old());
    assert!(EntrySource::Delete.in_old());
}

#[test]
fn entry_source_in_new() {
    assert!(EntrySource::OldOnly.in_new());
    assert!(EntrySource::WriteOnly.in_new());
    assert!(EntrySource::Both.in_new());
    assert!(!EntrySource::Delete.in_new());
}

#[test]
fn entry_source_in_write() {
    assert!(!EntrySource::OldOnly.in_write());
    assert!(EntrySource::WriteOnly.in_write());
    assert!(EntrySource::Both.in_write());
    assert!(EntrySource::Delete.in_write());
}

// ── Multi-segment gap tests ──

#[test]
fn valid_gap_in_second_segment() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_gap(0, 1, 5),
        sc_old_only(0, 1, 10, [2, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("gap in second segment should pass");
}

// ── u64::MAX edge cases ──

#[test]
fn valid_two_entries_ending_at_u64_max() {
    let rows = vec![
        sc_old_only(0, 0, 1, [1, 0, 0]),
        sc_old_only(0, 0, u64::MAX, [2, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("entries [1, u64::MAX] should pass");
}

#[test]
fn valid_u64_max_key_different_segments() {
    let rows = vec![
        sc_old_only(0, 0, u64::MAX, [1, 0, 0]),
        sc_old_only(0, 1, u64::MAX, [2, 0, 0]),
        sc_old_only(1, 0, u64::MAX, [3, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace)
        .expect("u64::MAX key in different segments should pass");
}

// ── Duplicate key soundness ──

#[test]
fn soundness_duplicate_key_in_segment() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 0, 20, [2, 0, 0]),
    ];
    let mut trace = generate_state_column_trace::<3>(&rows);
    let width = state_column_width::<3>();
    // Copy row 0's key to row 1 (duplicate)
    let key0 = {
        let cols0: &Cols = borrow_cols(&trace.values[0..width]);
        (
            cols0.key.limbs.limb0,
            cols0.key.limbs.limb1,
            cols0.key.limbs.limb2,
        )
    };
    let cols1: &mut Cols = borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols1.key.limbs.limb0 = key0.0;
    cols1.key.limbs.limb1 = key0.1;
    cols1.key.limbs.limb2 = key0.2;
    debug_check(&StateColumnChip::<3>, &trace)
        .expect_err("duplicate key should fail strict inequality");
}

// ── Single entry per segment ──

#[test]
fn valid_single_entry_per_segment() {
    let rows = vec![
        sc_old_only(0, 0, 10, [1, 0, 0]),
        sc_old_only(0, 1, 20, [2, 0, 0]),
        sc_old_only(1, 0, 30, [3, 0, 0]),
    ];
    let trace = generate_state_column_trace::<3>(&rows);
    debug_check(&StateColumnChip::<3>, &trace).expect("single entry per segment should pass");
}
