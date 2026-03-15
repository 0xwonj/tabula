//! Tests for the MetaShard AIR chip.

use p3_koala_bear::KoalaBear;

use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::scheme_tags;

use tabula_chips::shards::meta::air::MetaShardChip;
use tabula_chips::shards::meta::columns::META_SHARD_WIDTH;
use tabula_chips::shards::meta::trace::{MetaShardRow, generate_meta_shard_trace};
use tabula_stark::chips::ChipId;
use tabula_stark::debug::debug_check;

use tabula_chips::test_utils::builders::{
    ms_both_empty, ms_empty_to_nonempty, ms_touched, ms_untouched,
};
use tabula_chips::test_utils::values::{com_empty, distinct_digest};

fn chip() -> MetaShardChip {
    MetaShardChip::new(ChipId(100), 0, 0, scheme_tags::SSMC, true)
}

fn trace(row: Option<&MetaShardRow>) -> RowMajorMatrix<KoalaBear> {
    generate_meta_shard_trace(0, 0, scheme_tags::SSMC, row)
}

// ── Column width ──

#[test]
fn width_is_96() {
    assert_eq!(META_SHARD_WIDTH, 96);
}

// ── A. Valid traces ──

#[test]
fn valid_touched() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let r = ms_touched(d1, d2);
    debug_check(&chip(), &trace(Some(&r))).expect("touched should pass");
}

#[test]
fn valid_untouched() {
    let d1 = distinct_digest(1);
    let r = ms_untouched(d1);
    debug_check(&chip(), &trace(Some(&r))).expect("untouched should pass");
}

#[test]
fn valid_empty_column() {
    let d = com_empty(0, 0);
    let r = ms_both_empty(d);
    debug_check(&chip(), &trace(Some(&r))).expect("both empty should pass");
}

#[test]
fn valid_empty_to_nonempty() {
    let d_empty = com_empty(0, 0);
    let d_new = distinct_digest(1);
    let r = ms_empty_to_nonempty(d_empty, d_new);
    debug_check(&chip(), &trace(Some(&r))).expect("empty→non-empty should pass");
}

#[test]
fn valid_no_real_rows() {
    debug_check(&chip(), &trace(None)).expect("all-padding should pass");
}

#[test]
fn valid_smt_tag() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let r = ms_touched(d1, d2);
    // SMT scheme: scheme_tag=1, receives_commitment=false
    let c = MetaShardChip::new(ChipId(101), 0, 0, scheme_tags::SMT, false);
    let t = generate_meta_shard_trace(0, 0, scheme_tags::SMT, Some(&r));
    debug_check(&c, &t).expect("SMT tag should pass");
}

// ── B. Invalid traces ──

#[test]
fn invalid_untouched_com_mismatch() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let r = MetaShardRow {
        com_old: d1,
        com_new: d2, // different from com_old, but not touched
        is_empty_old: false,
        is_empty_new: false,
        is_touched: false,
        empty_read_count: 0,
    };
    debug_check(&chip(), &trace(Some(&r))).expect_err("untouched com mismatch should fail");
}

#[test]
fn invalid_untouched_empty_changed() {
    let d1 = distinct_digest(1);
    let r = MetaShardRow {
        com_old: d1,
        com_new: d1,
        is_empty_old: true,
        is_empty_new: false, // changed despite untouched
        is_touched: false,
        empty_read_count: 0,
    };
    debug_check(&chip(), &trace(Some(&r))).expect_err("untouched empty changed should fail");
}

#[test]
fn invalid_empty_stays_empty_when_touched() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let r = MetaShardRow {
        com_old: d1,
        com_new: d2,
        is_empty_old: true,
        is_empty_new: true, // should be 0 since touched + was empty
        is_touched: true,
        empty_read_count: 0,
    };
    debug_check(&chip(), &trace(Some(&r))).expect_err("empty_old=1 ∧ touched=1 ⟹ empty_new=0");
}

#[test]
fn invalid_com_empty_wrong_com_old() {
    let wrong = distinct_digest(99);
    let d_new = distinct_digest(1);
    let r = MetaShardRow {
        com_old: wrong, // not Com_empty(0, 0)
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
        empty_read_count: 0,
    };
    debug_check(&chip(), &trace(Some(&r)))
        .expect_err("wrong com_old with is_empty_old=1 should fail");
}

#[test]
fn invalid_com_empty_wrong_com_new() {
    let d_old = distinct_digest(1);
    let wrong = distinct_digest(99);
    let r = MetaShardRow {
        com_old: d_old,
        com_new: wrong, // not Com_empty(0, 0)
        is_empty_old: false,
        is_empty_new: true,
        is_touched: true,
        empty_read_count: 0,
    };
    debug_check(&chip(), &trace(Some(&r)))
        .expect_err("wrong com_new with is_empty_new=1 should fail");
}

#[test]
fn invalid_com_empty_wrong_table_col() {
    // Com_empty for (0,0) used for chip at (1,0) → should fail
    let wrong = com_empty(0, 0); // wrong (t,c)
    let d_new = distinct_digest(1);
    let r = MetaShardRow {
        com_old: wrong,
        com_new: d_new,
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
        empty_read_count: 0,
    };
    let c = MetaShardChip::new(ChipId(102), 1, 0, scheme_tags::SSMC, true);
    let t = generate_meta_shard_trace(1, 0, scheme_tags::SSMC, Some(&r));
    debug_check(&c, &t).expect_err("Com_empty for wrong (t,c) should fail");
}

// ── C. Different chip instances ──

#[test]
fn valid_different_table_col() {
    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let c = MetaShardChip::new(ChipId(103), 3, 7, scheme_tags::SSMC, true);
    let r = ms_touched(d1, d2);
    let t = generate_meta_shard_trace(3, 7, scheme_tags::SSMC, Some(&r));
    debug_check(&c, &t).expect("different (t,c) chip should pass");
}

#[test]
fn valid_com_empty_different_table_col() {
    let d = com_empty(3, 7);
    let d_new = distinct_digest(1);
    let c = MetaShardChip::new(ChipId(104), 3, 7, scheme_tags::SSMC, true);
    let r = ms_empty_to_nonempty(d, d_new);
    let t = generate_meta_shard_trace(3, 7, scheme_tags::SSMC, Some(&r));
    debug_check(&c, &t).expect("Com_empty for (3,7) should pass");
}
