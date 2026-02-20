//! Column layout for the GlobalSSMC AIR.
//!
//! The GlobalSSMC table holds sorted entries for SSMC-committed columns.
//! Rows are sorted by `(table_id, col_id, key)` with segments delimited
//! by `(table_id, col_id)` changes.
//!
//! Each segment represents one SSMC-committed column. Keys are strictly
//! increasing within a segment; boundary flags mark first/last entries.

use crate::air::columns::num_cols;
use crate::air::gadgets::{
    HashChainInput, KeyRangeChecked, LexOrderingDirection, OrderingRangeChecked, SameKeyDetection,
};

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
    /// Row key (u64 as 3 BabyBear limbs + half-decomposition for range checks).
    pub key: KeyRangeChecked<T>,
    /// Value field elements (Tier 1 ComEnc, non-null).
    pub value: [T; W],

    // ── Boundary ──
    /// First entry of `(t,c)` segment.
    pub is_first: T,
    /// Last entry of `(t,c)` segment.
    pub is_last: T,

    // ── Hash chain ──
    /// Running Poseidon hash chain accumulator (8 field elements).
    pub hash_acc: [T; 8],
    /// Hash chain Poseidon input (16 field elements).
    pub hash_chain: HashChainInput<T>,

    // ── Ordering ──
    /// Proves `key < next_key` within same segment (StrictIneq + halves for range checks).
    pub key_ordering: OrderingRangeChecked<T>,
    /// Same-(t,c) detection via IsZero gadgets + tc_changed flag.
    pub segment: SameKeyDetection<T>,

    // ── Lex ordering direction ──
    /// Lex ordering direction at segment boundaries (3 cols).
    pub lex: LexOrderingDirection<T>,

    // ── LogUp witness columns ──
    /// Multiplicity witness for SsmcMembership bus (C2 receive).
    /// 1 if this entry is looked up by a SortedMem init row, 0 otherwise.
    pub mult_witness: T,
    /// Per-segment flag: 1 if this segment's column is touched in the batch.
    /// Must be constant within a segment. Used by MergeOldList bus (C3 send).
    pub segment_is_touched: T,
}

/// Compute the width of GlobalSsmcCols for a given value width.
pub const fn ssmc_width<const W: usize>() -> usize {
    num_cols::<GlobalSsmcCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const SSMC_STANDARD_WIDTH: usize = ssmc_width::<3>();
