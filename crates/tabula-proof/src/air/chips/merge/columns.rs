//! Column layout for the GlobalMerge AIR.
//!
//! The GlobalMerge table proves 3-way merge correctness:
//! OldList + WriteSet → NewList for each touched SSMC-committed column.
//!
//! Rows sorted by `(table_id, col_id, key)` with segments delimited
//! by `(table_id, col_id)` changes.

use crate::air::columns::num_cols;
use crate::air::gadgets::{
    HashChainInput, KeyRangeChecked, LexOrderingDirection, OrderingRangeChecked, SameKeyDetection,
};

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
    /// Row key (u64 limbs + half-decomposition for range checks).
    pub key: KeyRangeChecked<T>,

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

    // ── Hash chain ──
    /// Running hash of NewList entries (8 field elements).
    pub hash_acc: [T; 8],
    /// Hash chain Poseidon input (16 field elements).
    pub hash_chain: HashChainInput<T>,
    /// 1 if this is the first `in_new=1` row of the segment.
    pub is_first_in_new: T,
    /// Running flag: 1 if any prior row in this segment had `in_new=1`.
    pub has_prev_in_new: T,

    // ── Segment boundary ──
    /// 1 if this is the last row of a `(t,c)` segment.
    pub is_last_segment: T,

    // ── Ordering ──
    /// Proves `key < next_key` within same segment (StrictIneq + halves for range checks).
    pub key_ordering: OrderingRangeChecked<T>,
    /// Same-(t,c) detection via IsZero gadgets + tc_changed flag.
    pub segment: SameKeyDetection<T>,

    // ── Lex ordering direction ──
    /// Lex ordering direction at segment boundaries (3 cols).
    pub lex: LexOrderingDirection<T>,
}

/// Compute the width of GlobalMergeCols for a given value width.
pub const fn merge_width<const W: usize>() -> usize {
    num_cols::<GlobalMergeCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const MERGE_STANDARD_WIDTH: usize = merge_width::<3>();
