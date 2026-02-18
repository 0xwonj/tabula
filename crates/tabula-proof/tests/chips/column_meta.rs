use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};

use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::column_meta::columns::COLUMN_META_WIDTH;
use tabula_proof::air::chips::column_meta::trace::generate_column_meta_trace;
use tabula_proof::air::debug_check;

use crate::common::builders::meta_entry;
use crate::common::values::distinct_digest;

// ── Valid traces ──

#[test]
fn valid_three_real_rows_plus_padding() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let d3 = distinct_digest(3);
    let d4 = distinct_digest(4);
    let d5 = distinct_digest(5);
    let d6 = distinct_digest(6);
    let metas = vec![
        meta_entry(0, 0, true, d1, d2),
        meta_entry(0, 1, true, d3, d4),
        meta_entry(1, 0, true, d5, d6),
    ];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("valid trace should pass");
}

#[test]
fn valid_single_real_row() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![meta_entry(0, 0, true, d1, d2)];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("single row should pass");
}

#[test]
fn valid_all_padding() {
    let metas: Vec<ColumnMeta> = vec![];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("all-padding should pass");
}

#[test]
fn valid_untouched_com_equal() {
    let d1 = distinct_digest(1);
    // Untouched: com_new = com_old.
    let metas = vec![meta_entry(0, 0, false, d1, d1)];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("untouched with equal coms should pass");
}

#[test]
fn valid_smt_tag() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Smt,
        com_old: d1,
        com_new: d2,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("SMT tag=1 is valid boolean");
}

#[test]
fn valid_many_rows_different_tables() {
    let metas: Vec<ColumnMeta> = (0..7)
        .map(|i| {
            meta_entry(
                i,
                0,
                true,
                distinct_digest(i * 2),
                distinct_digest(i * 2 + 1),
            )
        })
        .collect();
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("many different tables should pass");
}

#[test]
fn valid_many_cols_same_table() {
    let metas: Vec<ColumnMeta> = (0..5)
        .map(|i| {
            meta_entry(
                0,
                i as u16,
                true,
                distinct_digest(i * 2),
                distinct_digest(i * 2 + 1),
            )
        })
        .collect();
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("many cols same table should pass");
}

// ── Invalid traces ──

#[test]
fn invalid_boolean_tag_equals_2() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![meta_entry(0, 0, true, d1, d2)];
    let mut trace = generate_column_meta_trace(&metas, &Default::default());
    // Set tag = 2 (invalid boolean).
    let tag_offset = 3; // is_real(0), table_id(1), col_id(2), tag(3)
    trace.values[tag_offset] = BabyBear::TWO;
    debug_check(&ColumnMetaChip, &trace).expect_err("tag=2 should fail boolean check");
}

#[test]
fn invalid_is_real_prefix_zero_then_one() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let d3 = distinct_digest(3);
    let d4 = distinct_digest(4);
    let metas = vec![
        meta_entry(0, 0, true, d1, d2),
        meta_entry(0, 1, true, d3, d4),
    ];
    let mut trace = generate_column_meta_trace(&metas, &Default::default());
    // Set row 0 is_real = 0, row 1 is_real = 1 -> violates prefix.
    trace.values[0] = BabyBear::ZERO; // row 0, is_real
    debug_check(&ColumnMetaChip, &trace).expect_err("0->1 should fail is_real prefix");
}

#[test]
fn invalid_duplicate_table_col() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let d3 = distinct_digest(3);
    let d4 = distinct_digest(4);
    let metas = vec![
        meta_entry(0, 0, true, d1, d2),
        meta_entry(0, 0, true, d3, d4),
    ];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect_err("duplicate (t,c) should fail ordering");
}

#[test]
fn invalid_untouched_com_mismatch() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    // Untouched but com_new != com_old.
    let metas = vec![meta_entry(0, 0, false, d1, d2)];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect_err("untouched with different coms should fail");
}

// ── M8-5: Touched consistency + empty transition tests ──

#[test]
fn valid_untouched_empty_preserved() {
    let d1 = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d1,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("untouched empty preserved");
}

#[test]
fn valid_empty_to_nonempty_transition() {
    let d_empty = NativeDigest([BabyBear::ZERO; 8]);
    let d_new = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d_empty,
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("empty→non-empty transition");
}

#[test]
fn valid_nonempty_stays_nonempty() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d2,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("non-empty stays non-empty");
}

#[test]
fn invalid_untouched_empty_changed() {
    let d1 = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d1,
        is_empty_old: true,
        is_empty_new: false, // changed despite untouched!
        is_touched: false,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect_err("untouched but empty flag changed");
}

#[test]
fn invalid_empty_stays_empty_when_touched() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d2,
        is_empty_old: true,
        is_empty_new: true, // should be 0 since touched + was empty
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("empty_old=1 ∧ touched=1 ⟹ empty_new must be 0");
}

#[test]
fn invalid_is_zero_soundness_forged_table_diff() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let d3 = distinct_digest(3);
    let d4 = distinct_digest(4);
    let metas = vec![
        meta_entry(0, 0, true, d1, d2),
        meta_entry(1, 0, true, d3, d4),
    ];
    let mut trace = generate_column_meta_trace(&metas, &Default::default());

    // Column layout: is_real(0), table_id(1), col_id(2), tag(3),
    //   com_old[8](4-11), com_new[8](12-19),
    //   is_empty_old(20), is_empty_new(21), is_touched(22), has_sorted_mem(23),
    //   table_diff_iz.inv(24), table_diff_iz.is_zero(25),
    //   col_diff_iz.inv(26), col_diff_iz.is_zero(27)
    let table_diff_iz_is_zero_offset = 25;
    trace.values[table_diff_iz_is_zero_offset] = BabyBear::ONE;
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("forged is_zero should fail IsZero constraint (val*is_zero!=0)");
}

// ── Column width test ──

#[test]
fn column_meta_width() {
    assert_eq!(COLUMN_META_WIDTH, 28);
}
