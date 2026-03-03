//! Column layout for the StateColumn AIR.
//!
//! Unifies SSMC (old state commitment) and Merge (old + write → new)
//! into a single sorted table per `(t,c)` segment.
//!
//! Row types:
//! - **Entry**: old_only, write_only, both, or delete (source encoded by `s1,s0`)
//! - **Gap**: non-membership proof row (`is_gap=1`)
//!
//! Two parallel hash chains compute Com_old and Com_new.

use crate::air::columns::num_cols;
use crate::gadgets::{
    HashChainInput, KeyRangeChecked, LexOrderingDirection, OrderingRangeChecked, SameKeyDetection,
};

/// Column layout for the StateColumn AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct StateColumnCols<T, const W: usize> {
    // ── Identity (3) ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    // ── Key (11) ──
    /// Row key (u64 limbs + half-decomposition for range checks).
    pub key: KeyRangeChecked<T>,

    // ── Source encoding (3) ──
    /// 1 if this row is a gap (non-membership proof), 0 if entry.
    pub is_gap: T,
    /// Source bit 1: high bit of 2-bit entry source selector.
    pub s1: T,
    /// Source bit 0: low bit of 2-bit entry source selector.
    /// (s1,s0) for entries: (0,0)=old_only, (0,1)=write_only, (1,0)=both, (1,1)=delete.
    /// Gap rows: s1=s0=0 (constrained).
    pub s0: T,

    // ── Values (2W) ──
    /// Old value from base state (meaningful for old_only/both/delete; zero for write_only/gap).
    pub old_val: [T; W],
    /// New value for commitment (meaningful for old_only/write_only/both; zero for delete/gap).
    pub new_val: [T; W],

    // ── Segment flag (1) ──
    /// 1 if this segment's column is touched in the batch.
    /// Must be constant within a segment.
    pub segment_is_touched: T,

    // ── Old hash chain (24) ──
    /// Running Poseidon hash chain accumulator for Com_old (8 FE).
    pub old_hash_acc: [T; 8],
    /// Hash chain Poseidon input for old chain (16 FE).
    pub old_hash_chain: HashChainInput<T>,

    // ── New hash chain (24) ──
    /// Running Poseidon hash chain accumulator for Com_new (8 FE).
    pub new_hash_acc: [T; 8],
    /// Hash chain Poseidon input for new chain (16 FE).
    pub new_hash_chain: HashChainInput<T>,

    // ── Chain tracking flags (5) ──
    /// 1 if any prior row in this segment had `in_old=1`.
    pub has_prev_old_entry: T,
    /// 1 if this is the last `in_old=1` row of the segment.
    pub is_last_old_entry: T,
    /// Running flag: 1 after the last old entry (no more in_old rows allowed).
    pub past_last_old_entry: T,
    /// 1 if any prior row in this segment had `in_new=1`.
    pub has_prev_new_entry: T,
    /// 1 if this is the last `in_new=1` row of the segment.
    pub is_last_new_entry: T,
    /// Running OR of `in_write` within the segment.
    /// Used to enforce `segment_is_touched` <-> "has any write".
    pub write_seen_prefix: T,

    // ── Key ordering (13) ──
    /// Proves `key < next_key` within same segment (StrictIneq + halves + bits).
    pub key_ordering: OrderingRangeChecked<T>,

    // ── Segment detection (5) ──
    /// Same-(t,c) detection via IsZero gadgets + tc_changed flag.
    pub segment: SameKeyDetection<T>,

    // ── Lex ordering direction (3) ──
    /// Lex ordering direction at segment boundaries.
    pub lex: LexOrderingDirection<T>,

    // ── LogUp multiplicity witnesses (2) ──
    /// Multiplicity witness for ReadAccess bus receive (C1).
    /// Free witness — LogUp soundness suffices.
    pub read_mult_witness: T,
    /// Multiplicity witness for WriteAccess bus receive (C4).
    pub write_mult_witness: T,
}

/// Compute the width of StateColumnCols for a given value width.
pub const fn state_column_width<const W: usize>() -> usize {
    num_cols::<StateColumnCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const STATE_COLUMN_STANDARD_WIDTH: usize = state_column_width::<3>();
