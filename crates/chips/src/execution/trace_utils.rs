//! Pure arithmetic utilities for u64 <-> KoalaBear limb conversions.
//!
//! These functions have no dependency on `ExecutionCols` or trace layout —
//! they operate purely on field elements and integers.

use p3_field::PrimeField32;
use p3_koala_bear::KoalaBear;

use tabula_gadgets::integer::MASK_30;

/// Reconstruct a u64 from limb-encoded KoalaBear values.
pub(super) fn reconstruct_u64_from_limbs(limbs: &[KoalaBear]) -> u64 {
    let l0 = limbs.first().map_or(0, |v| v.as_canonical_u32() as u64);
    let l1 = limbs.get(1).map_or(0, |v| v.as_canonical_u32() as u64);
    let l2 = limbs.get(2).map_or(0, |v| v.as_canonical_u32() as u64);
    l0 | (l1 << 30) | (l2 << 60)
}

/// Extract the canonical u32 value from a KoalaBear element.
pub(super) fn koalabear_to_u32(x: KoalaBear) -> u32 {
    // KoalaBear stores values in Montgomery form internally.
    // Use the as_canonical_u32 method to get the actual value.
    x.as_canonical_u32()
}

/// Helper: create limb-encoded KoalaBear values from a u64.
pub fn u64_to_limbs(val: u64) -> [KoalaBear; 3] {
    [
        KoalaBear::new((val & MASK_30) as u32),
        KoalaBear::new(((val >> 30) & MASK_30) as u32),
        KoalaBear::new((val >> 60) as u32),
    ]
}

/// Helper: reconstruct u64 from limb KoalaBear values.
pub fn limbs_to_u64(limbs: &[KoalaBear; 3]) -> u64 {
    let l0 = limbs[0].as_canonical_u32() as u64;
    let l1 = limbs[1].as_canonical_u32() as u64;
    let l2 = limbs[2].as_canonical_u32() as u64;
    l0 | (l1 << 30) | (l2 << 60)
}

/// Helper: add two u64 values and return result as limbs.
pub fn u64_add_limbs(a: u64, b: u64) -> [KoalaBear; 3] {
    u64_to_limbs(a.wrapping_add(b))
}

/// Helper: subtract two u64 values and return result as limbs.
pub fn u64_sub_limbs(a: u64, b: u64) -> [KoalaBear; 3] {
    u64_to_limbs(a.wrapping_sub(b))
}
