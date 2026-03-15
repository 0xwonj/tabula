#![allow(missing_docs)]
use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::constrain_null_canon;
use tabula_gadgets::mem::constrain_mem_read;
use tabula_stark::air::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::debug::debug_check;

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
        let local_row = main.current_slice();
        let cols: &NullCanonTestCols<AB::Var> = borrow_cols(local_row);
        builder.assert_bool(cols.val_is_null);
        constrain_null_canon(builder, cols.val_is_null.into(), &cols.val);
    }
}

fn make_null_canon_trace(is_null: bool, val: [u32; 3]) -> RowMajorMatrix<KoalaBear> {
    let mut values = vec![KoalaBear::ZERO; 2 * NULL_CANON_WIDTH];
    let row: &mut NullCanonTestCols<KoalaBear> = borrow_cols_mut(&mut values[..NULL_CANON_WIDTH]);
    row.val_is_null = if is_null {
        KoalaBear::ONE
    } else {
        KoalaBear::ZERO
    };
    for (i, v) in val.iter().enumerate() {
        row.val[i] = KoalaBear::new(*v);
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
        let local_row = main.current_slice();
        let cols: &MemReadTestCols<AB::Var> = borrow_cols(local_row);
        constrain_mem_read(
            builder,
            &cols.val,
            cols.val_is_null,
            &cols.mem,
            cols.mem_is_null,
        );
    }
}

#[test]
fn mem_read_valid_match() {
    let mut values = vec![KoalaBear::ZERO; 2 * MEM_READ_WIDTH];
    let row: &mut MemReadTestCols<KoalaBear> = borrow_cols_mut(&mut values[..MEM_READ_WIDTH]);
    row.val = [KoalaBear::new(1), KoalaBear::new(2), KoalaBear::new(3)];
    row.val_is_null = KoalaBear::ZERO;
    row.mem = [KoalaBear::new(1), KoalaBear::new(2), KoalaBear::new(3)];
    row.mem_is_null = KoalaBear::ZERO;
    let trace = RowMajorMatrix::new(values, MEM_READ_WIDTH);
    debug_check(&MemReadTestChip, &trace).expect("matching read should pass");
}

#[test]
fn mem_read_invalid_mismatch() {
    let mut values = vec![KoalaBear::ZERO; 2 * MEM_READ_WIDTH];
    let row: &mut MemReadTestCols<KoalaBear> = borrow_cols_mut(&mut values[..MEM_READ_WIDTH]);
    row.val = [KoalaBear::new(1), KoalaBear::new(2), KoalaBear::new(3)];
    row.val_is_null = KoalaBear::ZERO;
    row.mem = [KoalaBear::new(1), KoalaBear::new(999), KoalaBear::new(3)];
    row.mem_is_null = KoalaBear::ZERO;
    let trace = RowMajorMatrix::new(values, MEM_READ_WIDTH);
    debug_check(&MemReadTestChip, &trace).expect_err("mismatched read should fail");
}
