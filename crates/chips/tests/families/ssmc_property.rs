//! Tests for the SSMC property AIR chip.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_chips::shards::property::SsmcPropertyChip;
use tabula_chips::shards::property::columns::{
    SSMC_PROPERTY_STANDARD_WIDTH, SsmcPropertyCols, ssmc_property_width,
};
use tabula_chips::test_utils::builders::{
    SsmcPropertyTestRow, generate_ssmc_property_test_trace, u64_limbs_vec,
};
use tabula_stark::air::borrow_cols_mut;
use tabula_stark::chips::ChipId;
use tabula_stark::debug::debug_check;

const _: () = assert!(SSMC_PROPERTY_STANDARD_WIDTH > 0);

fn chip() -> SsmcPropertyChip<3> {
    SsmcPropertyChip::new(ChipId(100), 0, 0)
}

fn successor_trace() -> RowMajorMatrix<KoalaBear> {
    generate_ssmc_property_test_trace::<3>(
        0,
        0,
        &[SsmcPropertyTestRow::Successor {
            query_key: 150,
            anchor_key: 200,
            prev_key: Some(100),
            result_val: u64_limbs_vec(75),
        }],
    )
}

fn predecessor_trace() -> RowMajorMatrix<KoalaBear> {
    generate_ssmc_property_test_trace::<3>(
        0,
        0,
        &[SsmcPropertyTestRow::Predecessor {
            query_key: 175,
            anchor_key: 100,
            next_key: Some(200),
            result_val: u64_limbs_vec(50),
        }],
    )
}

fn empty_successor_trace() -> RowMajorMatrix<KoalaBear> {
    generate_ssmc_property_test_trace::<3>(
        0,
        0,
        &[SsmcPropertyTestRow::EmptySuccessor { query_key: 999 }],
    )
}

#[test]
fn standard_width_matches_generic_width() {
    assert_eq!(SSMC_PROPERTY_STANDARD_WIDTH, ssmc_property_width::<3>());
}

#[test]
fn valid_single_successor_query() {
    debug_check(&chip(), &successor_trace()).expect("successor query should pass");
}

#[test]
fn valid_single_predecessor_query() {
    debug_check(&chip(), &predecessor_trace()).expect("predecessor query should pass");
}

#[test]
fn valid_empty_successor_query() {
    debug_check(&chip(), &empty_successor_trace()).expect("empty successor query should pass");
}

#[test]
fn invalid_is_real_not_boolean() {
    let width = ssmc_property_width::<3>();
    let mut values = vec![KoalaBear::ZERO; 2 * width];
    let cols: &mut SsmcPropertyCols<KoalaBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols.is_real = KoalaBear::TWO;

    let trace = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &trace).expect_err("is_real=2 should fail");
}

#[test]
fn invalid_query_selector_sum() {
    let width = ssmc_property_width::<3>();
    let mut trace = successor_trace();
    let row = trace.values.as_mut_slice();
    let cols: &mut SsmcPropertyCols<KoalaBear, 3> = borrow_cols_mut(&mut row[0..width]);
    cols.query_is_predecessor = KoalaBear::ONE;

    debug_check(&chip(), &trace).expect_err("multiple query selectors should fail");
}

#[test]
fn invalid_is_real_prefix_violation() {
    let width = ssmc_property_width::<3>();
    let mut values = vec![KoalaBear::ZERO; 4 * width];
    let cols1: &mut SsmcPropertyCols<KoalaBear, 3> = borrow_cols_mut(&mut values[width..2 * width]);
    cols1.is_real = KoalaBear::ONE;

    let trace = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &trace).expect_err("is_real prefix violation should fail");
}

#[test]
fn invalid_table_id_change() {
    let width = ssmc_property_width::<3>();
    let mut trace = generate_ssmc_property_test_trace::<3>(
        0,
        0,
        &[
            SsmcPropertyTestRow::Successor {
                query_key: 50,
                anchor_key: 100,
                prev_key: None,
                result_val: u64_limbs_vec(50),
            },
            SsmcPropertyTestRow::Predecessor {
                query_key: 250,
                anchor_key: 200,
                next_key: None,
                result_val: u64_limbs_vec(75),
            },
        ],
    );

    let values = trace.values.as_mut_slice();
    let cols1: &mut SsmcPropertyCols<KoalaBear, 3> = borrow_cols_mut(&mut values[width..2 * width]);
    cols1.table_id = KoalaBear::ONE;

    debug_check(&chip(), &trace).expect_err("table_id change should fail");
}

#[test]
fn invalid_successor_anchor_mismatch() {
    let width = ssmc_property_width::<3>();
    let mut trace = successor_trace();
    let values = trace.values.as_mut_slice();
    let cols: &mut SsmcPropertyCols<KoalaBear, 3> = borrow_cols_mut(&mut values[0..width]);
    cols.result_key.populate(999);

    debug_check(&chip(), &trace).expect_err("successor result/anchor mismatch should fail");
}
