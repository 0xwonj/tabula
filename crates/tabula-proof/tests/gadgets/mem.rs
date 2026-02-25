use p3_air::{Air, AirBuilder, BaseAir};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use tabula_proof::air::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_proof::debug::debug_check;
use tabula_proof::gadgets::constrain_null_canon;
use tabula_proof::gadgets::mem::constrain_mem_read;

// ── Null canonicality test chip ──

#[repr(C)]
struct NullCanonTestCols<T> {
    val_is_null: T,
    val: [T; 3],
}

const NULL_CANON_WIDTH: usize = num_cols::<NullCanonTestCols<u8>, u8>();

struct NullCanonTestChip;

impl<F> BaseAir<F> for NullCanonTestChip {
    fn width(&self) -> usize {
        NULL_CANON_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for NullCanonTestChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let row = main.row_slice(0).expect("row");
        let cols: &NullCanonTestCols<AB::Var> = borrow_cols(&row);
        builder.assert_bool(cols.val_is_null.clone());
        constrain_null_canon(builder, cols.val_is_null.clone().into(), &cols.val);
    }
}

fn make_null_canon_trace(is_null: bool, val: [u32; 3]) -> RowMajorMatrix<BabyBear> {
    let mut values = vec![BabyBear::ZERO; 2 * NULL_CANON_WIDTH];
    let row: &mut NullCanonTestCols<BabyBear> = borrow_cols_mut(&mut values[..NULL_CANON_WIDTH]);
    row.val_is_null = if is_null {
        BabyBear::ONE
    } else {
        BabyBear::ZERO
    };
    for (i, v) in val.iter().enumerate() {
        row.val[i] = BabyBear::new(*v);
    }
    RowMajorMatrix::new(values, NULL_CANON_WIDTH)
}

#[test]
fn null_canon_valid_null_with_zeros() {
    let trace = make_null_canon_trace(true, [0, 0, 0]);
    debug_check(&NullCanonTestChip, &trace).expect("null with zeros should pass");
}

#[test]
fn null_canon_valid_not_null_with_values() {
    let trace = make_null_canon_trace(false, [1, 2, 3]);
    debug_check(&NullCanonTestChip, &trace).expect("not-null with values should pass");
}

#[test]
fn null_canon_valid_not_null_with_zeros() {
    let trace = make_null_canon_trace(false, [0, 0, 0]);
    debug_check(&NullCanonTestChip, &trace).expect("not-null with zeros should pass");
}

#[test]
fn null_canon_invalid_null_with_nonzero() {
    let trace = make_null_canon_trace(true, [1, 0, 0]);
    debug_check(&NullCanonTestChip, &trace).expect_err("null with nonzero value should fail");
}

#[test]
fn null_canon_invalid_null_with_all_nonzero() {
    let trace = make_null_canon_trace(true, [1, 2, 3]);
    debug_check(&NullCanonTestChip, &trace).expect_err("null with all nonzero should fail");
}

// ── Memory read test chip ──

#[repr(C)]
struct MemReadTestCols<T> {
    val: [T; 3],
    val_is_null: T,
    mem: [T; 3],
    mem_is_null: T,
}

const MEM_READ_WIDTH: usize = num_cols::<MemReadTestCols<u8>, u8>();

struct MemReadTestChip;

impl<F> BaseAir<F> for MemReadTestChip {
    fn width(&self) -> usize {
        MEM_READ_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for MemReadTestChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let row = main.row_slice(0).expect("row");
        let cols: &MemReadTestCols<AB::Var> = borrow_cols(&row);
        constrain_mem_read(
            builder,
            &cols.val,
            cols.val_is_null.clone(),
            &cols.mem,
            cols.mem_is_null.clone(),
        );
    }
}

#[test]
fn mem_read_valid_match() {
    let mut values = vec![BabyBear::ZERO; 2 * MEM_READ_WIDTH];
    let row: &mut MemReadTestCols<BabyBear> = borrow_cols_mut(&mut values[..MEM_READ_WIDTH]);
    row.val = [BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)];
    row.val_is_null = BabyBear::ZERO;
    row.mem = [BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)];
    row.mem_is_null = BabyBear::ZERO;
    let trace = RowMajorMatrix::new(values, MEM_READ_WIDTH);
    debug_check(&MemReadTestChip, &trace).expect("matching read should pass");
}

#[test]
fn mem_read_invalid_mismatch() {
    let mut values = vec![BabyBear::ZERO; 2 * MEM_READ_WIDTH];
    let row: &mut MemReadTestCols<BabyBear> = borrow_cols_mut(&mut values[..MEM_READ_WIDTH]);
    row.val = [BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)];
    row.val_is_null = BabyBear::ZERO;
    row.mem = [BabyBear::new(1), BabyBear::new(999), BabyBear::new(3)];
    row.mem_is_null = BabyBear::ZERO;
    let trace = RowMajorMatrix::new(values, MEM_READ_WIDTH);
    debug_check(&MemReadTestChip, &trace).expect_err("mismatched read should fail");
}
