//! Column layout for the GlobalMerge AIR.
//!
//! The GlobalMerge table proves 3-way merge correctness:
//! OldList + WriteSet → NewList for each touched SSMC-committed column.
//!
//! Rows sorted by `(table_id, col_id, key)` with segments delimited
//! by `(table_id, col_id)` changes.

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, StrictIneq, U64Limbs};

/// Column layout for the GlobalMerge AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct GlobalMergeCols<T, const W: usize> {
    // ── Identity ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    // ── Merged key ──
    /// Strictly increasing row key within segment.
    pub key: U64Limbs<T>,

    // ── Source encoding ──
    /// Source bit 1: high bit of 2-bit source selector.
    pub s1: T,
    /// Source bit 0: low bit of 2-bit source selector.
    /// (s1,s0): (0,0)=old_only, (0,1)=write_only, (1,0)=both, (1,1)=delete.
    pub s0: T,

    // ── Values ──
    /// Old value from OldList (meaningful for old_only/both/delete).
    pub old_val: [T; W],
    /// Write value from WriteSet (meaningful for write_only/both/delete).
    pub write_val: [T; W],
    /// New value for NewList (result of merge).
    pub new_val: [T; W],

    // ── Flags ──
    /// 1 if this entry is in NewList, 0 if deleted.
    pub in_new: T,

    // ── Hash accumulator (NOT constrained in M8) ──
    /// Running hash of NewList entries (8 field elements).
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

/// Compute the width of GlobalMergeCols for a given value width.
pub const fn merge_width<const W: usize>() -> usize {
    num_cols::<GlobalMergeCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const MERGE_STANDARD_WIDTH: usize = merge_width::<3>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width() {
        // is_real(1) + table_id(1) + col_id(1) + key(3)
        // + s1(1) + s0(1) + old_val(3) + write_val(3) + new_val(3) + in_new(1)
        // + hash_acc(8) + key_ordering(3) + table_diff_iz(2) + col_diff_iz(2) + tc_changed(1)
        // = 1+1+1+3+1+1+3+3+3+1+8+3+2+2+1 = 34
        assert_eq!(MERGE_STANDARD_WIDTH, 34);
    }
}
