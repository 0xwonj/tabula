//! Ordering range-checked operation: StrictIneq + half-decomposition.
//!
//! Bundles a strict inequality proof (a < b) with limb half-decompositions
//! for range checking the gap `b - a - 1`.
//!
//! Used by SSMC (key_ordering), Merge (key_ordering), SortedMem (ordering).

use p3_air::AirBuilder;
use p3_baby_bear::BabyBear;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::integer::{
    Limb2Bits, LimbHalves, MASK_30, StrictIneq, constrain_limb_halves, constrain_limb2_bits,
};

/// Strict inequality with range-check half-decomposition.
///
/// Columns: 13 (StrictIneq(5) + LimbHalves(2) × 2 + Limb2Bits(4)).
///
/// Proves `a < b` for u64 values using a borrow-chain approach,
/// then range-checks: diff0/diff1 via halves, diff2 via 4-bit bits.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct OrderingRangeChecked<T> {
    /// StrictIneq gap: limbs + borrows of `b - a - 1`.
    pub ineq: StrictIneq<T>,
    /// Half-decomposition of ineq.diff0.
    pub diff0_halves: LimbHalves<T>,
    /// Half-decomposition of ineq.diff1.
    pub diff1_halves: LimbHalves<T>,
    /// 4-bit boolean decomposition of ineq.diff2 (proves diff2 ∈ [0, 16)).
    pub diff2_bits: Limb2Bits<T>,
}

impl OrderingRangeChecked<BabyBear> {
    /// Populate all columns proving `a < b`.
    ///
    /// # Panics
    /// Panics if `a >= b`.
    pub fn populate(&mut self, a: u64, b: u64) {
        self.ineq.populate(a, b);
        let gap = b - a - 1;
        let d0 = (gap & MASK_30) as u32;
        let d1 = ((gap >> 30) & MASK_30) as u32;
        let d2 = (gap >> 60) as u32;
        self.diff0_halves.populate(d0);
        self.diff1_halves.populate(d1);
        self.diff2_bits.populate(d2);
    }
}

/// Constrain decompositions: halves for diff0/diff1, bits for diff2.
pub fn constrain_ordering_halves<AB: AirBuilder>(
    builder: &mut AB,
    ordering: &OrderingRangeChecked<AB::Var>,
) {
    constrain_limb_halves(
        builder,
        ordering.ineq.diff0.clone().into(),
        &ordering.diff0_halves,
    );
    constrain_limb_halves(
        builder,
        ordering.ineq.diff1.clone().into(),
        &ordering.diff1_halves,
    );
    constrain_limb2_bits(
        builder,
        ordering.ineq.diff2.clone().into(),
        &ordering.diff2_bits,
    );
}

/// Emit all constraints and bus interactions for an OrderingRangeChecked gadget.
///
/// Combines [`constrain_ordering_halves`] + [`send_ordering_range_checks`] in one call.
/// Use this to ensure both structural constraints and bus sends are always applied together.
pub fn eval_ordering_range_checked<AB: InteractionAirBuilder>(
    builder: &mut AB,
    ordering: &OrderingRangeChecked<AB::Var>,
    mult: AB::Expr,
) {
    constrain_ordering_halves(builder, ordering);
    send_ordering_range_checks(builder, ordering, mult);
}

/// Send 4 range checks for an OrderingRangeChecked value.
///
/// Sends: diff0_lo, diff0_hi, diff1_lo, diff1_hi.
/// Note: diff2 is proven ∈ [0, 16) by Limb2Bits boolean decomposition, no RC send needed.
// AB::Expr is cloned for each bus send; by-value avoids one clone on the last send.
#[allow(clippy::needless_pass_by_value)]
pub fn send_ordering_range_checks<AB: InteractionAirBuilder>(
    builder: &mut AB,
    ordering: &OrderingRangeChecked<AB::Var>,
    mult: AB::Expr,
) {
    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_rc(ordering.diff0_halves.lo.clone().into());
    send_rc(ordering.diff0_halves.hi.clone().into());
    send_rc(ordering.diff1_halves.lo.clone().into());
    send_rc(ordering.diff1_halves.hi.clone().into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::PrimeCharacteristicRing;

    fn zero_ordering() -> OrderingRangeChecked<BabyBear> {
        OrderingRangeChecked {
            ineq: StrictIneq {
                diff0: BabyBear::ZERO,
                diff1: BabyBear::ZERO,
                diff2: BabyBear::ZERO,
                borrow0: BabyBear::ZERO,
                borrow1: BabyBear::ZERO,
            },
            diff0_halves: LimbHalves {
                lo: BabyBear::ZERO,
                hi: BabyBear::ZERO,
            },
            diff1_halves: LimbHalves {
                lo: BabyBear::ZERO,
                hi: BabyBear::ZERO,
            },
            diff2_bits: Limb2Bits {
                b0: BabyBear::ZERO,
                b1: BabyBear::ZERO,
                b2: BabyBear::ZERO,
                b3: BabyBear::ZERO,
            },
        }
    }

    #[test]
    fn ordering_rc_populate_adjacent() {
        let mut ord = zero_ordering();
        ord.populate(10, 11);
        // gap = 11 - 10 - 1 = 0
        assert_eq!(ord.ineq.diff0, BabyBear::ZERO);
        assert_eq!(ord.diff0_halves.lo, BabyBear::ZERO);
    }

    #[test]
    fn ordering_rc_populate_large_gap() {
        let mut ord = zero_ordering();
        let a = 100u64;
        let b = (1u64 << 31) + 200;
        ord.populate(a, b);
        let gap = b - a - 1;
        let d0 = (gap & MASK_30) as u32;
        assert_eq!(ord.ineq.diff0, BabyBear::new(d0));
        // Halves should reconstruct
        let lo = d0 & ((1 << 15) - 1);
        let hi = d0 >> 15;
        assert_eq!(ord.diff0_halves.lo, BabyBear::new(lo));
        assert_eq!(ord.diff0_halves.hi, BabyBear::new(hi));
    }

    #[test]
    #[should_panic(expected = "StrictIneq")]
    fn ordering_rc_populate_invalid() {
        let mut ord = zero_ordering();
        ord.populate(100, 50); // a >= b → panic
    }
}
