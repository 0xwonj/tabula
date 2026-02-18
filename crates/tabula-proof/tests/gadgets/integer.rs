use p3_air::{Air, AirBuilder, BaseAir};
use p3_baby_bear::BabyBear;
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use tabula_proof::air::{
    IsZero, StrictIneq, U64Limbs, borrow_cols, borrow_cols_mut, constrain_is_zero,
    constrain_strict_ineq, constrain_u64_decomposition, debug_check, num_cols,
};

// ── IsZero tests ──

#[repr(C)]
struct IsZeroTestCols<T> {
    val: T,
    iz: IsZero<T>,
}

const IS_ZERO_WIDTH: usize = num_cols::<IsZeroTestCols<u8>, u8>();

struct IsZeroTestChip;

impl<F> BaseAir<F> for IsZeroTestChip {
    fn width(&self) -> usize {
        IS_ZERO_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for IsZeroTestChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let row = main.row_slice(0).expect("row");
        let cols: &IsZeroTestCols<AB::Var> = borrow_cols(&row);
        constrain_is_zero(builder, cols.val.clone().into(), &cols.iz);
    }
}

fn make_is_zero_trace(val: BabyBear) -> RowMajorMatrix<BabyBear> {
    let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH];
    let row0: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
    row0.val = val;
    row0.iz.populate(val);
    let row1: &mut IsZeroTestCols<BabyBear> =
        borrow_cols_mut(&mut values[IS_ZERO_WIDTH..2 * IS_ZERO_WIDTH]);
    row1.val = BabyBear::ZERO;
    row1.iz.populate(BabyBear::ZERO);
    RowMajorMatrix::new(values, IS_ZERO_WIDTH)
}

#[test]
fn is_zero_with_zero_value() {
    let trace = make_is_zero_trace(BabyBear::ZERO);
    debug_check(&IsZeroTestChip, &trace).expect("zero should pass");
}

#[test]
fn is_zero_with_nonzero_value() {
    let trace = make_is_zero_trace(BabyBear::new(42));
    debug_check(&IsZeroTestChip, &trace).expect("nonzero should pass");
}

#[test]
fn is_zero_with_large_value() {
    let trace = make_is_zero_trace(BabyBear::new(BabyBear::ORDER_U32 - 1));
    debug_check(&IsZeroTestChip, &trace).expect("p-1 should pass");
}

#[test]
fn is_zero_soundness_claim_zero_on_nonzero() {
    let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH];
    let row: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
    row.val = BabyBear::new(42);
    row.iz.is_zero = BabyBear::ONE;
    row.iz.inv = BabyBear::ZERO;
    let trace = RowMajorMatrix::new(values, IS_ZERO_WIDTH);
    debug_check(&IsZeroTestChip, &trace).expect_err("should fail: val*is_zero != 0");
}

#[test]
fn is_zero_soundness_claim_nonzero_on_zero() {
    let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH];
    let row: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
    row.val = BabyBear::ZERO;
    row.iz.is_zero = BabyBear::ZERO;
    row.iz.inv = BabyBear::new(1);
    let trace = RowMajorMatrix::new(values, IS_ZERO_WIDTH);
    debug_check(&IsZeroTestChip, &trace).expect_err("should fail: (1-is_zero)*(1-val*inv) != 0");
}

// ── U64Limbs tests ──

#[repr(C)]
struct U64TestCols<T> {
    expected: T,
    limbs: U64Limbs<T>,
}

const U64_WIDTH: usize = num_cols::<U64TestCols<u8>, u8>();

struct U64TestChip;

impl<F> BaseAir<F> for U64TestChip {
    fn width(&self) -> usize {
        U64_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for U64TestChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let row = main.row_slice(0).expect("row");
        let cols: &U64TestCols<AB::Var> = borrow_cols(&row);
        constrain_u64_decomposition(builder, &cols.limbs, cols.expected.clone().into());
    }
}

fn make_u64_trace(val: u64) -> RowMajorMatrix<BabyBear> {
    let shift_30_u32: u32 = 1 << 30;
    let mut values = vec![BabyBear::ZERO; 2 * U64_WIDTH];
    let row: &mut U64TestCols<BabyBear> = borrow_cols_mut(&mut values[..U64_WIDTH]);
    let shift_30 = BabyBear::new(shift_30_u32);
    let shift_60 = shift_30 * shift_30;
    row.limbs.populate(val);
    row.expected = row.limbs.limb0 + row.limbs.limb1 * shift_30 + row.limbs.limb2 * shift_60;
    RowMajorMatrix::new(values, U64_WIDTH)
}

#[test]
fn u64_limbs_zero() {
    let trace = make_u64_trace(0);
    debug_check(&U64TestChip, &trace).expect("zero should pass");
}

#[test]
fn u64_limbs_max() {
    let trace = make_u64_trace(u64::MAX);
    debug_check(&U64TestChip, &trace).expect("max should pass");
}

#[test]
fn u64_limbs_mid_value() {
    let trace = make_u64_trace(1_000_000_000);
    debug_check(&U64TestChip, &trace).expect("mid should pass");
}

#[test]
fn u64_limbs_soundness_wrong_limb() {
    let shift_30_u32: u32 = 1 << 30;
    let mut values = vec![BabyBear::ZERO; 2 * U64_WIDTH];
    let row: &mut U64TestCols<BabyBear> = borrow_cols_mut(&mut values[..U64_WIDTH]);
    let val = 42u64;
    let shift_30 = BabyBear::new(shift_30_u32);
    let shift_60 = shift_30 * shift_30;
    row.limbs.populate(val);
    row.expected = row.limbs.limb0 + row.limbs.limb1 * shift_30 + row.limbs.limb2 * shift_60;
    row.limbs.limb0 = BabyBear::new(999);
    let trace = RowMajorMatrix::new(values, U64_WIDTH);
    debug_check(&U64TestChip, &trace).expect_err("corrupted limb should fail");
}

// ── StrictIneq tests ──

#[repr(C)]
struct IneqTestCols<T> {
    is_real: T,
    a: U64Limbs<T>,
    b: U64Limbs<T>,
    ineq: StrictIneq<T>,
}

const INEQ_WIDTH: usize = num_cols::<IneqTestCols<u8>, u8>();

struct IneqTestChip;

impl<F> BaseAir<F> for IneqTestChip {
    fn width(&self) -> usize {
        INEQ_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for IneqTestChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let row = main.row_slice(0).expect("row");
        let cols: &IneqTestCols<AB::Var> = borrow_cols(&row);
        let mut when_real = builder.when(cols.is_real.clone());
        constrain_strict_ineq(&mut when_real, &cols.a, &cols.b, &cols.ineq);
    }
}

fn make_ineq_trace(a: u64, b: u64) -> RowMajorMatrix<BabyBear> {
    let mut values = vec![BabyBear::ZERO; 2 * INEQ_WIDTH];
    let row: &mut IneqTestCols<BabyBear> = borrow_cols_mut(&mut values[..INEQ_WIDTH]);
    row.is_real = BabyBear::ONE;
    row.a.populate(a);
    row.b.populate(b);
    row.ineq.populate(a, b);
    RowMajorMatrix::new(values, INEQ_WIDTH)
}

#[test]
fn strict_ineq_adjacent() {
    let trace = make_ineq_trace(0, 1);
    debug_check(&IneqTestChip, &trace).expect("0 < 1");
}

#[test]
fn strict_ineq_large_gap() {
    let trace = make_ineq_trace(0, u64::MAX);
    debug_check(&IneqTestChip, &trace).expect("0 < MAX");
}

#[test]
fn strict_ineq_near_max() {
    let trace = make_ineq_trace(u64::MAX - 1, u64::MAX);
    debug_check(&IneqTestChip, &trace).expect("MAX-1 < MAX");
}

#[test]
fn strict_ineq_mid_values() {
    let trace = make_ineq_trace(1_000, 2_000_000);
    debug_check(&IneqTestChip, &trace).expect("1000 < 2000000");
}

#[test]
fn strict_ineq_cross_limb_boundary() {
    let trace = make_ineq_trace((1 << 30) - 1, 1 << 30);
    debug_check(&IneqTestChip, &trace).expect("2^30-1 < 2^30");
}

#[test]
fn strict_ineq_soundness_wrong_gap() {
    let mut values = vec![BabyBear::ZERO; 2 * INEQ_WIDTH];
    let row: &mut IneqTestCols<BabyBear> = borrow_cols_mut(&mut values[..INEQ_WIDTH]);
    row.is_real = BabyBear::ONE;
    row.a.populate(10);
    row.b.populate(20);
    row.ineq.populate(10, 20);
    row.ineq.diff0 = BabyBear::new(999);
    let trace = RowMajorMatrix::new(values, INEQ_WIDTH);
    debug_check(&IneqTestChip, &trace).expect_err("corrupted gap should fail");
}

#[test]
#[should_panic(expected = "must be < b")]
fn strict_ineq_populate_panics_on_equal() {
    let mut ineq = StrictIneq {
        diff0: BabyBear::ZERO,
        diff1: BabyBear::ZERO,
        diff2: BabyBear::ZERO,
    };
    ineq.populate(5, 5);
}

#[test]
#[should_panic(expected = "must be < b")]
fn strict_ineq_populate_panics_on_reversed() {
    let mut ineq = StrictIneq {
        diff0: BabyBear::ZERO,
        diff1: BabyBear::ZERO,
        diff2: BabyBear::ZERO,
    };
    ineq.populate(10, 5);
}
