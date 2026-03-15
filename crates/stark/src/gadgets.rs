//! Minimal gadget types needed by the bus macro framework.
//!
//! Full gadget constraint implementations live in `tabula-gadgets`.
//! Only types referenced by [`define_bus!`] macro expansions live here.

use p3_field::PrimeField32;
use p3_koala_bear::KoalaBear;

/// 30-bit mask for limb extraction.
pub const MASK_30: u64 = (1 << 30) - 1;

/// 2^30 as u32 (fits in KoalaBear: 1073741824 < p = 2130706433).
pub const SHIFT_30_U32: u32 = 1 << 30;

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

impl U64Limbs<KoalaBear> {
    /// Fill limb columns from a u64 value.
    pub fn populate(&mut self, val: u64) {
        self.limb0 = KoalaBear::new((val & MASK_30) as u32);
        self.limb1 = KoalaBear::new(((val >> 30) & MASK_30) as u32);
        self.limb2 = KoalaBear::new((val >> 60) as u32);
    }

    /// Create U64Limbs from a u64 value.
    pub fn from_u64(val: u64) -> Self {
        Self {
            limb0: KoalaBear::new((val & MASK_30) as u32),
            limb1: KoalaBear::new(((val >> 30) & MASK_30) as u32),
            limb2: KoalaBear::new((val >> 60) as u32),
        }
    }

    /// Reconstruct the u64 value from limbs.
    pub fn to_u64(&self) -> u64 {
        let l0 = self.limb0.as_canonical_u32() as u64;
        let l1 = self.limb1.as_canonical_u32() as u64;
        let l2 = self.limb2.as_canonical_u32() as u64;
        l0 | (l1 << 30) | (l2 << 60)
    }
}
