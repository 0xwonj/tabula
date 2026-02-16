//! Integer gadgets: U64 limb decomposition, strict inequality, and is-zero.
//!
//! Each gadget provides:
//! - A `#[repr(C)]` column struct embeddable in any chip's columns
//! - A `populate()` function for witness generation
//! - A `constrain()` function for AIR constraint emission

use p3_air::AirBuilder;
use p3_baby_bear::BabyBear;
use p3_field::integers::QuotientMap;
use p3_field::{Field, PrimeCharacteristicRing};

/// 30-bit mask for limb extraction.
pub(crate) const MASK_30: u64 = (1 << 30) - 1;

/// 2^30 as u32 (fits in BabyBear: 1073741824 < p = 2013265921).
pub(crate) const SHIFT_30_U32: u32 = 1 << 30;

/// Create an `AB::Expr` from a u32 constant in generic AIR context.
pub(crate) fn expr_from_u32<AB: AirBuilder>(val: u32) -> AB::Expr {
    let prime_val =
        <<AB::Expr as PrimeCharacteristicRing>::PrimeSubfield as QuotientMap<u32>>::from_int(val);
    AB::Expr::from_prime_subfield(prime_val)
}

// ── U64Limbs ──────────────────────────────────────────────────────────────────

/// 3-limb decomposition of a u64 (30+30+4 bits).
///
/// - `limb0`: bits [0..30), range [0, 2^30)
/// - `limb1`: bits [30..60), range [0, 2^30)
/// - `limb2`: bits [60..64), range [0, 16)
///
/// Reconstruction: `val = limb0 + limb1 * 2^30 + limb2 * 2^60`.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct U64Limbs<T> {
    /// Bits [0..30).
    pub limb0: T,
    /// Bits [30..60).
    pub limb1: T,
    /// Bits [60..64).
    pub limb2: T,
}

impl U64Limbs<BabyBear> {
    /// Fill limb columns from a u64 value.
    pub fn populate(&mut self, val: u64) {
        self.limb0 = BabyBear::new((val & MASK_30) as u32);
        self.limb1 = BabyBear::new(((val >> 30) & MASK_30) as u32);
        self.limb2 = BabyBear::new((val >> 60) as u32);
    }
}

/// Constrain that limbs reconstruct to the expected value.
///
/// Emits: `expected - (limb0 + limb1 * 2^30 + limb2 * 2^60) = 0`
///
/// **Range checks on individual limbs are declared via LogUp (wired in M9).**
/// Without range checks, a prover could use out-of-range limbs that reconstruct
/// to the correct value modulo p. Callers must ensure limbs are range-checked.
pub fn constrain_u64_decomposition<AB: AirBuilder>(
    builder: &mut AB,
    limbs: &U64Limbs<AB::Var>,
    expected: AB::Expr,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();

    let reconstructed: AB::Expr = limbs.limb0.clone().into()
        + limbs.limb1.clone().into() * shift_30
        + limbs.limb2.clone().into() * shift_60;

    builder.assert_eq(expected, reconstructed);
}

// ── IsZero ────────────────────────────────────────────────────────────────────

/// Is-zero gadget: determines whether a field element is zero.
///
/// - `inv`: inverse of the value (arbitrary when value = 0)
/// - `is_zero`: boolean flag (1 if value = 0, 0 otherwise)
///
/// Constraints:
/// 1. `is_zero` is boolean
/// 2. `val * is_zero = 0` (if is_zero=1 then val=0)
/// 3. `(1 - is_zero) * (1 - val * inv) = 0` (if is_zero=0 then val has inverse)
#[repr(C)]
#[derive(Clone, Debug)]
pub struct IsZero<T> {
    /// Inverse of the value (arbitrary when value = 0).
    pub inv: T,
    /// 1 if value = 0, 0 otherwise.
    pub is_zero: T,
}

impl IsZero<BabyBear> {
    /// Fill witness columns from a field element.
    pub fn populate(&mut self, val: BabyBear) {
        if val == BabyBear::ZERO {
            self.inv = BabyBear::ZERO;
            self.is_zero = BabyBear::ONE;
        } else {
            self.inv = val.inverse();
            self.is_zero = BabyBear::ZERO;
        }
    }
}

/// Constrain the is-zero relationship: `is_zero = (val == 0)`.
///
/// Emits 3 constraints:
/// 1. `is_zero ∈ {0, 1}`
/// 2. `val * is_zero = 0`
/// 3. `(1 - is_zero) * (1 - val * inv) = 0`
pub fn constrain_is_zero<AB: AirBuilder>(builder: &mut AB, val: AB::Expr, iz: &IsZero<AB::Var>) {
    builder.assert_bool(iz.is_zero.clone());
    // val * is_zero = 0
    builder.assert_zero(val.clone() * iz.is_zero.clone().into());
    // (1 - is_zero) * (1 - val * inv) = 0
    let not_zero: AB::Expr = AB::Expr::ONE - iz.is_zero.clone().into();
    let has_inv: AB::Expr = AB::Expr::ONE - val * iz.inv.clone().into();
    builder.assert_zero(not_zero * has_inv);
}

// ── StrictIneq ────────────────────────────────────────────────────────────────

/// Strict inequality gadget for u64 values: proves `a < b`.
///
/// Since `a < b` iff `b - a - 1 >= 0` and fits in 64 bits, we decompose
/// `b - a - 1` into the same 30+30+4 limb format.
///
/// Columns: `diff0, diff1, diff2` — limbs of `b - a - 1`.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct StrictIneq<T> {
    /// Limb 0 of `b - a - 1`.
    pub diff0: T,
    /// Limb 1 of `b - a - 1`.
    pub diff1: T,
    /// Limb 2 of `b - a - 1`.
    pub diff2: T,
}

impl StrictIneq<BabyBear> {
    /// Fill witness columns proving `a < b`.
    ///
    /// # Panics
    /// Panics if `a >= b` (the inequality does not hold).
    pub fn populate(&mut self, a: u64, b: u64) {
        assert!(a < b, "StrictIneq: a ({a}) must be < b ({b})");
        let gap = b - a - 1;
        self.diff0 = BabyBear::new((gap & MASK_30) as u32);
        self.diff1 = BabyBear::new(((gap >> 30) & MASK_30) as u32);
        self.diff2 = BabyBear::new((gap >> 60) as u32);
    }
}

/// Constrain that `a < b` for u64 values represented as U64Limbs.
///
/// Emits: `b_reconstructed - a_reconstructed - 1 = diff0 + diff1*2^30 + diff2*2^60`
///
/// The diff limbs must be range-checked separately (via RangeCheck bus in M9).
/// - `diff0, diff1 ∈ [0, 2^30)` — via two 15-bit sub-limbs each
/// - `diff2 ∈ [0, 16)` — single range check
pub fn constrain_strict_ineq<AB: AirBuilder>(
    builder: &mut AB,
    a: &U64Limbs<AB::Var>,
    b: &U64Limbs<AB::Var>,
    ineq: &StrictIneq<AB::Var>,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();

    let a_val: AB::Expr = a.limb0.clone().into()
        + a.limb1.clone().into() * shift_30.clone()
        + a.limb2.clone().into() * shift_60.clone();

    let b_val: AB::Expr = b.limb0.clone().into()
        + b.limb1.clone().into() * shift_30.clone()
        + b.limb2.clone().into() * shift_60.clone();

    let gap_reconstructed: AB::Expr = ineq.diff0.clone().into()
        + ineq.diff1.clone().into() * shift_30
        + ineq.diff2.clone().into() * shift_60;

    // b - a - 1 = gap
    builder.assert_eq(b_val - a_val - AB::Expr::ONE, gap_reconstructed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
    use crate::air::debug::debug_check;
    use p3_air::{Air, BaseAir};
    use p3_field::PrimeField32;
    use p3_matrix::Matrix;
    use p3_matrix::dense::RowMajorMatrix;

    // ── IsZero tests ──

    /// Minimal chip for testing IsZero in isolation.
    /// Layout: [val, IsZero(inv, is_zero)] = 3 columns.
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
        let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH]; // 2 rows min
        // Row 0: the actual test row.
        let row0: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
        row0.val = val;
        row0.iz.populate(val);
        // Row 1 (padding): val=0, populate IsZero consistently.
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
        // Corrupt: val=42 but is_zero=1
        let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH];
        let row: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
        row.val = BabyBear::new(42);
        row.iz.is_zero = BabyBear::ONE; // wrong!
        row.iz.inv = BabyBear::ZERO;
        let trace = RowMajorMatrix::new(values, IS_ZERO_WIDTH);
        debug_check(&IsZeroTestChip, &trace).expect_err("should fail: val*is_zero != 0");
    }

    #[test]
    fn is_zero_soundness_claim_nonzero_on_zero() {
        // Corrupt: val=0 but is_zero=0 (no valid inv exists)
        let mut values = vec![BabyBear::ZERO; 2 * IS_ZERO_WIDTH];
        let row: &mut IsZeroTestCols<BabyBear> = borrow_cols_mut(&mut values[..IS_ZERO_WIDTH]);
        row.val = BabyBear::ZERO;
        row.iz.is_zero = BabyBear::ZERO; // wrong!
        row.iz.inv = BabyBear::new(1); // any value
        let trace = RowMajorMatrix::new(values, IS_ZERO_WIDTH);
        debug_check(&IsZeroTestChip, &trace)
            .expect_err("should fail: (1-is_zero)*(1-val*inv) != 0");
    }

    // ── U64Limbs tests ──

    /// Minimal chip for testing U64Limbs decomposition.
    /// Layout: [expected_val, U64Limbs(3)] = 4 columns.
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
        let mut values = vec![BabyBear::ZERO; 2 * U64_WIDTH];
        let row: &mut U64TestCols<BabyBear> = borrow_cols_mut(&mut values[..U64_WIDTH]);
        // Encode expected as field element via limb reconstruction:
        // expected = limb0 + limb1 * 2^30 + limb2 * 2^60 (in the field)
        let shift_30 = BabyBear::new(SHIFT_30_U32);
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
        let mut values = vec![BabyBear::ZERO; 2 * U64_WIDTH];
        let row: &mut U64TestCols<BabyBear> = borrow_cols_mut(&mut values[..U64_WIDTH]);
        let val = 42u64;
        let shift_30 = BabyBear::new(SHIFT_30_U32);
        let shift_60 = shift_30 * shift_30;
        row.limbs.populate(val);
        row.expected = row.limbs.limb0 + row.limbs.limb1 * shift_30 + row.limbs.limb2 * shift_60;
        // Corrupt limb0
        row.limbs.limb0 = BabyBear::new(999);
        let trace = RowMajorMatrix::new(values, U64_WIDTH);
        debug_check(&U64TestChip, &trace).expect_err("corrupted limb should fail");
    }

    // ── StrictIneq tests ──

    /// Minimal chip for testing StrictIneq.
    /// Layout: [is_real, U64Limbs(a), U64Limbs(b), StrictIneq(5)] = 12 columns.
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
            // Gate on is_real so padding rows don't trigger the constraint.
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
        // Row 1 (padding): is_real = 0, constraint inactive.
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
        // Values that cross the 30-bit boundary
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
        // Corrupt: change diff0 to wrong value
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
}
