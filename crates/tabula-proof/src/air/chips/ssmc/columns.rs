//! Column layout for the GlobalSSMC AIR.
//!
//! The GlobalSSMC table holds sorted entries for SSMC-committed columns.
//! Rows are sorted by `(table_id, col_id, key)` with segments delimited
//! by `(table_id, col_id)` changes.
//!
//! Each segment represents one SSMC-committed column. Keys are strictly
//! increasing within a segment; boundary flags mark first/last entries.

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, StrictIneq, U64Limbs};

/// Column layout for the GlobalSSMC AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64). Narrow: W=1 (Bool). Wide: W=8 (Digest).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct GlobalSsmcCols<T, const W: usize> {
    // ── Identity ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    // ── Entry ──
    /// Row key (u64 as 3 BabyBear limbs), strictly increasing within segment.
    pub key: U64Limbs<T>,
    /// Value field elements (Tier 1 ComEnc, non-null).
    pub value: [T; W],

    // ── Boundary ──
    /// First entry of `(t,c)` segment.
    pub is_first: T,
    /// Last entry of `(t,c)` segment.
    pub is_last: T,

    // ── Hash accumulator (populated, NOT constrained in M8) ──
    /// Running Poseidon hash chain accumulator (8 field elements).
    pub hash_acc: [T; 8],

    // ── Ordering gadgets ──
    /// Proves `key < next_key` within same segment.
    pub key_ordering: StrictIneq<T>,
    /// IsZero for `(next.table_id - table_id)`.
    pub table_diff_iz: IsZero<T>,
    /// IsZero for `(next.col_id - col_id)`.
    pub col_diff_iz: IsZero<T>,
    /// 1 if `(table_id, col_id)` changes from this row to the next.
    pub tc_changed: T,
}

/// Compute the width of GlobalSsmcCols for a given value width.
pub const fn ssmc_width<const W: usize>() -> usize {
    num_cols::<GlobalSsmcCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const SSMC_STANDARD_WIDTH: usize = ssmc_width::<3>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width() {
        // is_real(1) + table_id(1) + col_id(1) + key(3) + value(3)
        // + is_first(1) + is_last(1) + hash_acc(8)
        // + key_ordering(3) + table_diff_iz(2) + col_diff_iz(2) + tc_changed(1)
        // = 1+1+1+3+3+1+1+8+3+2+2+1 = 27
        assert_eq!(SSMC_STANDARD_WIDTH, 27);
    }
}
