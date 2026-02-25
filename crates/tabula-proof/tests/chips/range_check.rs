use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_proof::air::borrow_cols;
use tabula_proof::chips::range_check::{
    RANGE_CHECK_SIZE, RANGE_CHECK_WIDTH, RangeCheckChip, RangeCheckCols,
    generate_range_check_preprocessed,
};
use tabula_proof::debug::debug_check;

#[test]
fn range_check_width_is_two() {
    assert_eq!(RANGE_CHECK_WIDTH, 2);
}

#[test]
fn preprocessed_table_size() {
    let trace = generate_range_check_preprocessed();
    assert_eq!(trace.height(), RANGE_CHECK_SIZE);
    assert_eq!(trace.width, RANGE_CHECK_WIDTH);
}

#[test]
fn preprocessed_table_values() {
    let trace = generate_range_check_preprocessed();
    // Spot check first, last, and middle
    let row_0: &RangeCheckCols<BabyBear> = borrow_cols(&trace.values[0..RANGE_CHECK_WIDTH]);
    assert_eq!(row_0.value, BabyBear::ZERO);

    let last = RANGE_CHECK_SIZE - 1;
    let offset = last * RANGE_CHECK_WIDTH;
    let row_last: &RangeCheckCols<BabyBear> =
        borrow_cols(&trace.values[offset..offset + RANGE_CHECK_WIDTH]);
    assert_eq!(row_last.value, BabyBear::new(last as u32));
}

#[test]
fn preprocessed_table_multiplicities_zero() {
    let trace = generate_range_check_preprocessed();
    for i in 0..RANGE_CHECK_SIZE {
        let offset = i * RANGE_CHECK_WIDTH;
        let row: &RangeCheckCols<BabyBear> =
            borrow_cols(&trace.values[offset..offset + RANGE_CHECK_WIDTH]);
        assert_eq!(row.multiplicity, BabyBear::ZERO);
    }
}

#[test]
fn range_check_chip_no_constraints() {
    // The chip has no constraints, so any trace should pass.
    let trace = generate_range_check_preprocessed();
    debug_check(&RangeCheckChip, &trace).expect("no constraints to fail");
}
