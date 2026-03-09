use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, scheme_tags};
use tabula_core::{ColId, TableId};

use tabula_chips::column_meta::air::ColumnMetaChip;
use tabula_chips::column_meta::columns::COLUMN_META_WIDTH;
use tabula_chips::column_meta::trace::generate_column_meta_trace;
use tabula_stark::debug::debug_check;

use tabula_chips::test_utils::builders::meta_entry;
use tabula_chips::test_utils::values::{com_empty, distinct_digest};

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
        tag: scheme_tags::SMT,
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
    let d = com_empty(0, 0);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: d,
        com_new: d,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("untouched empty preserved");
}

#[test]
fn valid_empty_to_nonempty_transition() {
    let d_empty = com_empty(0, 0);
    let d_new = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: scheme_tags::SSMC,
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
        tag: scheme_tags::SSMC,
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
        tag: scheme_tags::SSMC,
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
        tag: scheme_tags::SSMC,
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
    //   is_empty_old(20), is_empty_new(21), is_touched(22), empty_read_mult(23),
    //   table_diff_iz.inv(24), table_diff_iz.is_zero(25),
    //   col_diff_iz.inv(26), col_diff_iz.is_zero(27)
    let table_diff_iz_is_zero_offset = 25;
    trace.values[table_diff_iz_is_zero_offset] = BabyBear::ONE;
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("forged is_zero should fail IsZero constraint (val*is_zero!=0)");
}

// ── M10-B4: Com_empty verification tests ──

#[test]
fn valid_com_empty_both_empty() {
    // Both is_empty_old=1 and is_empty_new=1 (untouched empty column).
    // Both com_old and com_new must equal Com_empty.
    let d = com_empty(2, 5);
    let metas = vec![ColumnMeta {
        table: TableId(2),
        col: ColId(5),
        tag: scheme_tags::SSMC,
        com_old: d,
        com_new: d,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("both empty with correct Com_empty");
}

#[test]
fn invalid_com_empty_wrong_com_old() {
    // is_empty_old=1 but com_old is wrong → should fail.
    let wrong = distinct_digest(99);
    let d_new = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: wrong, // not Com_empty(0, 0)!
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("wrong com_old with is_empty_old=1 should fail");
}

#[test]
fn invalid_com_empty_wrong_com_new() {
    // is_empty_new=1 but com_new is wrong → should fail.
    let d_old = distinct_digest(1);
    let wrong = distinct_digest(99);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: d_old,
        com_new: wrong, // not Com_empty(0, 0)!
        is_empty_old: false,
        is_empty_new: true,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("wrong com_new with is_empty_new=1 should fail");
}

#[test]
fn valid_com_empty_not_empty_arbitrary_com() {
    // Neither is_empty_old nor is_empty_new → Com_empty constraint doesn't apply.
    // Arbitrary commitments are fine.
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: d1,
        com_new: d2,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("non-empty rows don't need Com_empty match");
}

#[test]
fn valid_com_empty_different_table_col() {
    // Com_empty is (t,c)-specific. Verify with different table/col values.
    let d = com_empty(3, 7);
    let d_new = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(3),
        col: ColId(7),
        tag: scheme_tags::SSMC,
        com_old: d,
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect("Com_empty for (3,7) should pass");
}

#[test]
fn invalid_com_empty_wrong_table_col() {
    // Com_empty for (0,0) used as com_old for (1,0) → should fail.
    let wrong = com_empty(0, 0); // wrong (t,c)!
    let d_new = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: wrong,
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace).expect_err("Com_empty for wrong (t,c) should fail");
}

// ── T15: ColumnMeta mixed empty/non-empty ──

/// T15: Mix of empty and non-empty columns in the same trace must pass.
///
/// This verifies that the ColumnMeta chip correctly handles multiple rows with
/// different is_empty_old/is_empty_new combinations without interfering.
#[test]
fn valid_mixed_empty_and_nonempty_columns() {
    let d_empty_00 = com_empty(0, 0);
    let d_empty_01 = com_empty(0, 1);
    let d_new_00 = distinct_digest(10);
    let d_old_10 = distinct_digest(20);
    let d_new_10 = distinct_digest(21);

    let metas = vec![
        // (t=0, c=0): was empty, now has data after being touched.
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: scheme_tags::SSMC,
            com_old: d_empty_00,
            com_new: d_new_00,
            is_empty_old: true,
            is_empty_new: false,
            is_touched: true,
        },
        // (t=0, c=1): was empty, still empty (untouched).
        ColumnMeta {
            table: TableId(0),
            col: ColId(1),
            tag: scheme_tags::SSMC,
            com_old: d_empty_01,
            com_new: d_empty_01,
            is_empty_old: true,
            is_empty_new: true,
            is_touched: false,
        },
        // (t=1, c=0): non-empty → non-empty after update.
        ColumnMeta {
            table: TableId(1),
            col: ColId(0),
            tag: scheme_tags::SSMC,
            com_old: d_old_10,
            com_new: d_new_10,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        },
    ];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect("mixed empty/non-empty columns in same trace should pass");
}

/// T15b: Mixed with wrong Com_empty in empty-old row must fail.
#[test]
fn invalid_mixed_wrong_com_empty_in_empty_row() {
    let d_wrong_for_00 = com_empty(0, 99); // wrong (t,c) — valid Com_empty but for (0,99) not (0,0)
    let d_new_00 = distinct_digest(10);
    let d_old_10 = distinct_digest(20);
    let d_new_10 = distinct_digest(21);

    let metas = vec![
        // (t=0, c=0): is_empty_old=true but wrong com_old (uses (0,99) instead of (0,0)).
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: scheme_tags::SSMC,
            com_old: d_wrong_for_00,
            com_new: d_new_00,
            is_empty_old: true,
            is_empty_new: false,
            is_touched: true,
        },
        // (t=1, c=0): correct non-empty row.
        ColumnMeta {
            table: TableId(1),
            col: ColId(0),
            tag: scheme_tags::SSMC,
            com_old: d_old_10,
            com_new: d_new_10,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        },
    ];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect_err("wrong Com_empty in mixed trace must fail Com_empty verification");
}

/// T15c: Column with is_empty_old=false but untouched (com_old=com_new) passes.
/// Mix this with a touched empty→non-empty column.
#[test]
fn valid_mixed_untouched_nonempty_and_touched_empty() {
    let d_empty_00 = com_empty(0, 0);
    let d_new_00 = distinct_digest(5);
    let d_nonempty_01 = distinct_digest(30);

    let metas = vec![
        // (t=0, c=0): empty → non-empty (touched).
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: scheme_tags::SSMC,
            com_old: d_empty_00,
            com_new: d_new_00,
            is_empty_old: true,
            is_empty_new: false,
            is_touched: true,
        },
        // (t=0, c=1): non-empty, untouched (com_old=com_new).
        ColumnMeta {
            table: TableId(0),
            col: ColId(1),
            tag: scheme_tags::SSMC,
            com_old: d_nonempty_01,
            com_new: d_nonempty_01,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
    ];
    let trace = generate_column_meta_trace(&metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect("untouched non-empty next to touched empty should pass");
}

// ── Column width test ──

#[test]
fn column_meta_width() {
    assert_eq!(COLUMN_META_WIDTH, 104);
}
