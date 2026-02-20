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

// ── LimbHalves ───────────────────────────────────────────────────────────────

/// 15-bit mask for half-limb extraction.
pub(crate) const MASK_15: u64 = (1 << 15) - 1;

/// 2^15 as u32 (fits in BabyBear).
pub(crate) const SHIFT_15_U32: u32 = 1 << 15;

/// Half-decomposition of a 30-bit limb into two 15-bit halves.
///
/// - `lo`: bits [0..15), range [0, 2^15)
/// - `hi`: bits [15..30), range [0, 2^15)
///
/// Reconstruction: `limb = lo + hi * 2^15`.
///
/// Each half fits in [0, 2^16) and is sent to the RangeCheck bus.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct LimbHalves<T> {
    /// Lower 15 bits of the limb.
    pub lo: T,
    /// Upper 15 bits of the limb.
    pub hi: T,
}

impl LimbHalves<BabyBear> {
    /// Fill half columns from a 30-bit limb value.
    pub fn populate(&mut self, limb_val: u32) {
        debug_assert!(limb_val < (1 << 30), "limb value must be < 2^30");
        self.lo = BabyBear::new(limb_val & MASK_15 as u32);
        self.hi = BabyBear::new(limb_val >> 15);
    }
}

/// Constrain that `limb = halves.lo + halves.hi * 2^15`.
pub fn constrain_limb_halves<AB: AirBuilder>(
    builder: &mut AB,
    limb: AB::Expr,
    halves: &LimbHalves<AB::Var>,
) {
    let shift_15: AB::Expr = expr_from_u32::<AB>(SHIFT_15_U32);
    let reconstructed: AB::Expr = halves.lo.clone().into() + halves.hi.clone().into() * shift_15;
    builder.assert_eq(limb, reconstructed);
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

// ── Limb2Bits ────────────────────────────────────────────────────────────────

/// 4-bit boolean decomposition for the top limb of a u64 (limb2 ∈ [0, 16)).
///
/// Columns: 4 boolean bits.
/// Reconstruction: `val = b0 + 2*b1 + 4*b2 + 8*b3`.
///
/// This is necessary because the RangeCheck bus only validates [0, 2^16),
/// which is too loose for the 4-bit top limb.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct Limb2Bits<T> {
    /// Bit 0 (LSB).
    pub b0: T,
    /// Bit 1.
    pub b1: T,
    /// Bit 2.
    pub b2: T,
    /// Bit 3 (MSB).
    pub b3: T,
}

impl Limb2Bits<BabyBear> {
    /// Fill bit columns from a 4-bit value.
    pub fn populate(&mut self, val: u32) {
        debug_assert!(val < 16, "Limb2Bits: value must be < 16, got {val}");
        self.b0 = BabyBear::new(val & 1);
        self.b1 = BabyBear::new((val >> 1) & 1);
        self.b2 = BabyBear::new((val >> 2) & 1);
        self.b3 = BabyBear::new((val >> 3) & 1);
    }
}

/// Constrain 4-bit boolean decomposition: `val = b0 + 2*b1 + 4*b2 + 8*b3`.
///
/// Emits 5 constraints: 4 boolean + 1 reconstruction.
pub fn constrain_limb2_bits<AB: AirBuilder>(
    builder: &mut AB,
    val: AB::Expr,
    bits: &Limb2Bits<AB::Var>,
) {
    builder.assert_bool(bits.b0.clone());
    builder.assert_bool(bits.b1.clone());
    builder.assert_bool(bits.b2.clone());
    builder.assert_bool(bits.b3.clone());
    let reconstructed: AB::Expr = bits.b0.clone().into()
        + bits.b1.clone().into() * AB::Expr::TWO
        + bits.b2.clone().into() * AB::Expr::from_u8(4)
        + bits.b3.clone().into() * AB::Expr::from_u8(8);
    builder.assert_eq(val, reconstructed);
}

// ── StrictIneq ────────────────────────────────────────────────────────────────

/// Strict inequality gadget for u64 values: proves `a < b`.
///
/// Uses a borrow-chain approach for per-limb subtraction, avoiding
/// field-arithmetic overflow that would make a single-equation approach unsound.
///
/// Columns: `diff0, diff1, diff2` (limbs of `b - a - 1`) + `borrow0, borrow1`.
///
/// Per-limb equations:
/// - `diff0 = b.limb0 - a.limb0 - 1 + borrow0 * 2^30`
/// - `diff1 = b.limb1 - a.limb1 - borrow0 + borrow1 * 2^30`
/// - `diff2 = b.limb2 - a.limb2 - borrow1`
#[repr(C)]
#[derive(Clone, Debug)]
pub struct StrictIneq<T> {
    /// Limb 0 of `b - a - 1`.
    pub diff0: T,
    /// Limb 1 of `b - a - 1`.
    pub diff1: T,
    /// Limb 2 of `b - a - 1`.
    pub diff2: T,
    /// Borrow from limb 0 to limb 1 (boolean).
    pub borrow0: T,
    /// Borrow from limb 1 to limb 2 (boolean).
    pub borrow1: T,
}

impl StrictIneq<BabyBear> {
    /// Fill witness columns proving `a < b`.
    ///
    /// # Panics
    /// Panics if `a >= b` (the inequality does not hold).
    pub fn populate(&mut self, a: u64, b: u64) {
        assert!(a < b, "StrictIneq: a ({a}) must be < b ({b})");

        let a0 = (a & MASK_30) as i64;
        let a1 = ((a >> 30) & MASK_30) as i64;
        let a2 = (a >> 60) as i64;
        let b0 = (b & MASK_30) as i64;
        let b1 = ((b >> 30) & MASK_30) as i64;
        let b2 = (b >> 60) as i64;

        let shift = 1i64 << 30;

        let temp0 = b0 - a0 - 1;
        let borrow0 = if temp0 < 0 { 1i64 } else { 0i64 };
        let d0 = temp0 + borrow0 * shift;

        let temp1 = b1 - a1 - borrow0;
        let borrow1 = if temp1 < 0 { 1i64 } else { 0i64 };
        let d1 = temp1 + borrow1 * shift;

        let d2 = b2 - a2 - borrow1;
        debug_assert!(d0 >= 0 && d0 < shift, "diff0 out of range: {d0}");
        debug_assert!(d1 >= 0 && d1 < shift, "diff1 out of range: {d1}");
        debug_assert!((0..16).contains(&d2), "diff2 out of range: {d2}");

        self.diff0 = BabyBear::new(d0 as u32);
        self.diff1 = BabyBear::new(d1 as u32);
        self.diff2 = BabyBear::new(d2 as u32);
        self.borrow0 = BabyBear::new(borrow0 as u32);
        self.borrow1 = BabyBear::new(borrow1 as u32);
    }
}

/// Constrain that `a < b` for u64 values represented as U64Limbs.
///
/// Uses borrow-chain per-limb equations. Borrows are boolean-constrained.
/// The diff limbs must be range-checked separately:
/// - `diff0, diff1 ∈ [0, 2^30)` — via LimbHalves in OrderingRangeChecked
/// - `diff2 ∈ [0, 16)` — via Limb2Bits in OrderingRangeChecked
pub fn constrain_strict_ineq<AB: AirBuilder>(
    builder: &mut AB,
    a: &U64Limbs<AB::Var>,
    b: &U64Limbs<AB::Var>,
    ineq: &StrictIneq<AB::Var>,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    builder.assert_bool(ineq.borrow0.clone());
    builder.assert_bool(ineq.borrow1.clone());

    // Limb 0: diff0 = b.limb0 - a.limb0 - 1 + borrow0 * 2^30
    let expected0: AB::Expr = b.limb0.clone().into() - a.limb0.clone().into() - AB::Expr::ONE
        + ineq.borrow0.clone().into() * shift_30.clone();
    builder.assert_eq(ineq.diff0.clone().into(), expected0);

    // Limb 1: diff1 = b.limb1 - a.limb1 - borrow0 + borrow1 * 2^30
    let expected1: AB::Expr = b.limb1.clone().into()
        - a.limb1.clone().into()
        - ineq.borrow0.clone().into()
        + ineq.borrow1.clone().into() * shift_30;
    builder.assert_eq(ineq.diff1.clone().into(), expected1);

    // Limb 2: diff2 = b.limb2 - a.limb2 - borrow1
    let expected2: AB::Expr =
        b.limb2.clone().into() - a.limb2.clone().into() - ineq.borrow1.clone().into();
    builder.assert_eq(ineq.diff2.clone().into(), expected2);
}
