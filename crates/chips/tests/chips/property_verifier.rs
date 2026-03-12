//! Tests for the PropertyVerifier AIR chip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use tabula_chips::shards::property::air::PropertyVerifierChip;
use tabula_chips::shards::property::columns::{
    PROPERTY_VERIFIER_STANDARD_WIDTH, PropertyVerifierCols, property_verifier_width,
};
use tabula_chips::shards::property::trace::{PropertyReadRecord, generate_property_verifier_trace};
use tabula_stark::air::borrow_cols_mut;
use tabula_stark::chips::ChipId;
use tabula_stark::debug::debug_check;

fn chip() -> PropertyVerifierChip<3> {
    PropertyVerifierChip::new(ChipId(100), 0, 0)
}

fn trace(records: &[PropertyReadRecord]) -> RowMajorMatrix<BabyBear> {
    generate_property_verifier_trace::<3>(0, 0, records)
}

fn record(query_type: u8, key: u64, val: u64, is_null: bool) -> PropertyReadRecord {
    PropertyReadRecord {
        query_type,
        result_val: vec![BabyBear::new(val as u32), BabyBear::ZERO, BabyBear::ZERO],
        result_key: vec![BabyBear::new(key as u32), BabyBear::ZERO, BabyBear::ZERO],
        is_null,
    }
}

// ── Column width ──

#[test]
fn standard_width_is_11() {
    assert_eq!(PROPERTY_VERIFIER_STANDARD_WIDTH, 11);
}

#[test]
fn generic_width_matches() {
    assert_eq!(property_verifier_width::<3>(), 11);
}

// ── A. Valid single-query traces ──

#[test]
fn valid_single_minimum_query() {
    let records = vec![record(0, 100, 50, false)];
    debug_check(&chip(), &trace(&records)).expect("single MIN query should pass");
}

#[test]
fn valid_single_maximum_query() {
    let records = vec![record(1, 200, 75, false)];
    debug_check(&chip(), &trace(&records)).expect("single MAX query should pass");
}

#[test]
fn valid_single_successor_query() {
    let records = vec![record(2, 300, 90, false)];
    debug_check(&chip(), &trace(&records)).expect("single Successor query should pass");
}

#[test]
fn valid_single_predecessor_query() {
    let records = vec![record(3, 400, 10, false)];
    debug_check(&chip(), &trace(&records)).expect("single Predecessor query should pass");
}

#[test]
fn valid_null_result() {
    let records = vec![record(0, 0, 0, true)];
    debug_check(&chip(), &trace(&records)).expect("null result should pass");
}

// ── B. Valid multi-query traces ──

#[test]
fn valid_two_different_queries() {
    let records = vec![record(0, 100, 50, false), record(1, 200, 75, false)];
    debug_check(&chip(), &trace(&records)).expect("two different queries should pass");
}

#[test]
fn valid_multiple_same_type_queries() {
    let records = vec![
        record(0, 100, 50, false),
        record(0, 200, 75, false),
        record(0, 300, 90, false),
    ];
    debug_check(&chip(), &trace(&records)).expect("multiple same type should pass");
}

#[test]
fn valid_mixed_null_and_nonnull() {
    let records = vec![
        record(0, 100, 50, false),
        record(1, 0, 0, true),
        record(2, 300, 90, false),
    ];
    debug_check(&chip(), &trace(&records)).expect("mixed null/nonnull should pass");
}

#[test]
fn valid_empty_records() {
    let records: Vec<PropertyReadRecord> = vec![];
    debug_check(&chip(), &trace(&records)).expect("all-padding should pass");
}

// ── C. Invalid traces ──

#[test]
fn invalid_is_real_not_boolean() {
    let width = property_verifier_width::<3>();
    let mut values = vec![BabyBear::ZERO; 2 * width];

    let cols: &mut PropertyVerifierCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols.is_real = BabyBear::TWO; // NOT boolean

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("is_real=2 should fail");
}

#[test]
fn invalid_is_null_not_boolean() {
    let width = property_verifier_width::<3>();
    let mut values = vec![BabyBear::ZERO; 2 * width];

    let cols: &mut PropertyVerifierCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols.is_real = BabyBear::ONE;
    cols.is_null = BabyBear::TWO; // NOT boolean

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("is_null=2 should fail");
}

#[test]
fn invalid_is_real_prefix_violation() {
    // Row 0: is_real=0, Row 1: is_real=1 → violates monotonic 1→0 prefix
    let width = property_verifier_width::<3>();
    let mut values = vec![BabyBear::ZERO; 4 * width];

    // Row 0: padding (is_real=0)
    // Row 1: real (is_real=1) → prefix violation
    let cols1: &mut PropertyVerifierCols<BabyBear, 3> =
        borrow_cols_mut(&mut values[width..2 * width]);
    cols1.is_real = BabyBear::ONE;

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("prefix violation should fail");
}

#[test]
fn invalid_table_id_change() {
    let width = property_verifier_width::<3>();
    let mut values = vec![BabyBear::ZERO; 4 * width];

    // Row 0: table_id=0
    let cols0: &mut PropertyVerifierCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols0.is_real = BabyBear::ONE;
    cols0.table_id = BabyBear::ZERO;

    // Row 1: table_id=1 (VIOLATION)
    let cols1: &mut PropertyVerifierCols<BabyBear, 3> =
        borrow_cols_mut(&mut values[width..2 * width]);
    cols1.is_real = BabyBear::ONE;
    cols1.table_id = BabyBear::ONE;

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("table_id change should fail");
}

#[test]
fn invalid_col_id_change() {
    let width = property_verifier_width::<3>();
    let mut values = vec![BabyBear::ZERO; 4 * width];

    // Row 0: col_id=0
    let cols0: &mut PropertyVerifierCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols0.is_real = BabyBear::ONE;
    cols0.col_id = BabyBear::ZERO;

    // Row 1: col_id=5 (VIOLATION)
    let cols1: &mut PropertyVerifierCols<BabyBear, 3> =
        borrow_cols_mut(&mut values[width..2 * width]);
    cols1.is_real = BabyBear::ONE;
    cols1.col_id = BabyBear::new(5);

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("col_id change should fail");
}

// ── D. Different chip instances ──

#[test]
fn valid_different_table_col() {
    let c = PropertyVerifierChip::<3>::new(ChipId(101), 7, 3);
    let records = vec![record(0, 100, 50, false)];
    let t = generate_property_verifier_trace::<3>(7, 3, &records);
    debug_check(&c, &t).expect("different (t,c) chip should pass");
}

#[test]
fn valid_large_table_col_ids() {
    let c = PropertyVerifierChip::<3>::new(ChipId(102), 999, 255);
    let records = vec![record(1, 500, 42, false), record(2, 600, 0, true)];
    let t = generate_property_verifier_trace::<3>(999, 255, &records);
    debug_check(&c, &t).expect("large (t,c) chip should pass");
}

// ── E. Trace generation properties ──

#[test]
fn trace_pads_to_power_of_two() {
    let records = vec![record(0, 100, 50, false)];
    let t = trace(&records);
    // 1 real row + 1 → 2 → padded to 2 (already power of 2)
    assert_eq!(t.height(), 2);

    let records = vec![record(0, 100, 50, false), record(1, 200, 75, false)];
    let t = trace(&records);
    // 2 real rows + 1 → 3 → padded to 4
    assert_eq!(t.height(), 4);
}

#[test]
fn trace_minimum_two_rows() {
    let records: Vec<PropertyReadRecord> = vec![];
    let t = trace(&records);
    assert!(t.height() >= 2);
}
