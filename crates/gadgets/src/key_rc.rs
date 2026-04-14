//! Key range-checked operation: U64Limbs + half-decomposition.
//!
//! Bundles a u64 value (3 limbs) with its limb half-decompositions for
//! range checking via the RangeCheck LogUp bus.
//!
//! Used for SSMC/Merge keys, SortedMem r/tau, Execution access_r/tau_rc.

use p3_air::AirBuilder;
use p3_field::PrimeField32;
use p3_koala_bear::KoalaBear;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::integer::{
    Limb2Bits, LimbHalves, MASK_30, U64Limbs, constrain_limb_halves, constrain_limb2_bits,
};

/// U64 value with range-check half-decomposition.
///
/// Columns: 11 (U64Limbs(3) + LimbHalves(2) × 2 + Limb2Bits(4)).
///
/// The three limbs are 30+30+4 bits. Limbs 0 and 1 are half-decomposed
/// into two 15-bit halves each. Limb2 (4-bit) is boolean-decomposed.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct KeyRangeChecked<T> {
    /// The u64 value as 3 KoalaBear limbs (30+30+4).
    pub limbs: U64Limbs<T>,
    /// Half-decomposition of limbs.limb0.
    pub l0_halves: LimbHalves<T>,
    /// Half-decomposition of limbs.limb1.
    pub l1_halves: LimbHalves<T>,
    /// 4-bit boolean decomposition of limbs.limb2 (proves limb2 ∈ [0, 16)).
    pub limb2_bits: Limb2Bits<T>,
}

impl KeyRangeChecked<KoalaBear> {
    /// Populate all columns from a u64 value.
    pub fn populate(&mut self, val: u64) {
        self.limbs.populate(val);
        let l0 = (val & MASK_30) as u32;
        let l1 = ((val >> 30) & MASK_30) as u32;
        let l2 = (val >> 60) as u32;
        self.l0_halves.populate(l0);
        self.l1_halves.populate(l1);
        self.limb2_bits.populate(l2);
    }

    /// Populate all columns from a native committed-key payload.
    pub fn populate_payload(&mut self, payload: &[KoalaBear; 3]) {
        let limb0 = payload[0].as_canonical_u32() as u64;
        let limb1 = payload[1].as_canonical_u32() as u64;
        let limb2 = payload[2].as_canonical_u32() as u64;
        self.limbs.limb0 = payload[0];
        self.limbs.limb1 = payload[1];
        self.limbs.limb2 = payload[2];
        self.l0_halves.populate(limb0 as u32);
        self.l1_halves.populate(limb1 as u32);
        self.limb2_bits.populate(limb2 as u32);
    }
}

/// Constrain limb decompositions: halves for limb0/1, bits for limb2.
pub fn constrain_key_halves<AB: AirBuilder>(builder: &mut AB, key: &KeyRangeChecked<AB::Var>) {
    constrain_limb_halves(builder, key.limbs.limb0.into(), &key.l0_halves);
    constrain_limb_halves(builder, key.limbs.limb1.into(), &key.l1_halves);
    constrain_limb2_bits(builder, key.limbs.limb2.into(), &key.limb2_bits);
}

/// Emit all constraints and bus interactions for a KeyRangeChecked gadget.
///
/// Combines [`constrain_key_halves`] + [`send_key_range_checks`] in one call.
/// Use this to ensure both structural constraints and bus sends are always applied together.
pub fn eval_key_range_checked<AB: InteractionAirBuilder>(
    builder: &mut AB,
    key: &KeyRangeChecked<AB::Var>,
    mult: AB::Expr,
) {
    constrain_key_halves(builder, key);
    send_key_range_checks(builder, key, mult);
}

/// Send 4 range checks for a KeyRangeChecked value.
///
/// Sends: l0_lo, l0_hi, l1_lo, l1_hi.
/// Note: limb2 is proven ∈ [0, 16) by Limb2Bits boolean decomposition, no RC send needed.
// AB::Expr is cloned for each bus send; by-value avoids one clone on the last send.
#[allow(clippy::needless_pass_by_value)]
pub fn send_key_range_checks<AB: InteractionAirBuilder>(
    builder: &mut AB,
    key: &KeyRangeChecked<AB::Var>,
    mult: AB::Expr,
) {
    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_rc(key.l0_halves.lo.into());
    send_rc(key.l0_halves.hi.into());
    send_rc(key.l1_halves.lo.into());
    send_rc(key.l1_halves.hi.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::PrimeCharacteristicRing;

    #[test]
    fn key_rc_populate_zero() {
        let mut key = KeyRangeChecked {
            limbs: U64Limbs {
                limb0: KoalaBear::ZERO,
                limb1: KoalaBear::ZERO,
                limb2: KoalaBear::ZERO,
            },
            l0_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            l1_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            limb2_bits: Limb2Bits {
                b0: KoalaBear::ZERO,
                b1: KoalaBear::ZERO,
                b2: KoalaBear::ZERO,
                b3: KoalaBear::ZERO,
            },
        };
        key.populate(0);
        assert_eq!(key.limbs.limb0, KoalaBear::ZERO);
        assert_eq!(key.l0_halves.lo, KoalaBear::ZERO);
        assert_eq!(key.l0_halves.hi, KoalaBear::ZERO);
    }

    #[test]
    fn key_rc_populate_value() {
        let mut key = KeyRangeChecked {
            limbs: U64Limbs {
                limb0: KoalaBear::ZERO,
                limb1: KoalaBear::ZERO,
                limb2: KoalaBear::ZERO,
            },
            l0_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            l1_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            limb2_bits: Limb2Bits {
                b0: KoalaBear::ZERO,
                b1: KoalaBear::ZERO,
                b2: KoalaBear::ZERO,
                b3: KoalaBear::ZERO,
            },
        };
        let val: u64 = (1 << 15) + 42; // l0 = 42 + 2^15, halves: lo=42, hi=1
        key.populate(val);
        assert_eq!(key.l0_halves.lo, KoalaBear::new(42));
        assert_eq!(key.l0_halves.hi, KoalaBear::new(1));
    }

    #[test]
    fn key_rc_populate_large() {
        let mut key = KeyRangeChecked {
            limbs: U64Limbs {
                limb0: KoalaBear::ZERO,
                limb1: KoalaBear::ZERO,
                limb2: KoalaBear::ZERO,
            },
            l0_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            l1_halves: LimbHalves {
                lo: KoalaBear::ZERO,
                hi: KoalaBear::ZERO,
            },
            limb2_bits: Limb2Bits {
                b0: KoalaBear::ZERO,
                b1: KoalaBear::ZERO,
                b2: KoalaBear::ZERO,
                b3: KoalaBear::ZERO,
            },
        };
        let val: u64 = u64::MAX;
        key.populate(val);
        // limb2 = val >> 60 = 15
        assert_eq!(key.limbs.limb2, KoalaBear::new(15));
        // limb0 = val & MASK_30 = 2^30 - 1
        let mask30 = (1u64 << 30) - 1;
        assert_eq!(key.limbs.limb0, KoalaBear::new(mask30 as u32));
        // l0_halves: lo = (2^30-1) & 0x7FFF = 2^15-1, hi = (2^30-1) >> 15 = 2^15-1
        assert_eq!(key.l0_halves.lo, KoalaBear::new((1 << 15) - 1));
        assert_eq!(key.l0_halves.hi, KoalaBear::new((1 << 15) - 1));
    }
}
