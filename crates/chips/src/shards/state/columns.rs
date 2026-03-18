//! Column layout for the StateShard AIR.
//!
//! Per-column version of `StateColumnCols`, operating on a single `(t, c)`.
//! Row types: entry (old_only/write_only/both/delete) or gap (non-membership).
//! Two parallel hash chains compute Com_old and Com_new.
//!
//! Compared to the global `StateColumnCols`:
//! - Removed `SameKeyDetection` (5 cols): no segment boundaries
//! - Removed `LexOrderingDirection` (3 cols): no cross-segment ordering
//!
//! Column budget: 93 (W=3) before property-anchor multiplicity.

use tabula_gadgets::{HashChainInput, KeyRangeChecked, OrderingRangeChecked};
use tabula_stark::air::columns::num_cols;

/// Column layout for the StateShard AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct StateShardCols<T, const W: usize> {
    // ── Identity (3) ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier (constant across all rows).
    pub table_id: T,
    /// Column identifier (constant across all rows).
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
    /// 1 if this column is touched in the batch. Constant across all rows.
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

    // ── Chain tracking flags (6) ──
    /// 1 if any prior row had `in_old=1`.
    pub has_prev_old_entry: T,
    /// Previous `in_old=1` key, or zero when none exists yet.
    pub prev_old_key: KeyRangeChecked<T>,
    /// 1 if this is the last `in_old=1` row.
    pub is_last_old_entry: T,
    /// Next `in_old=1` key, or zero when this row has no later old entry.
    pub next_old_key: KeyRangeChecked<T>,
    /// Running flag: 1 after the last old entry (no more in_old rows allowed).
    pub past_last_old_entry: T,
    /// 1 if any prior row had `in_new=1`.
    pub has_prev_new_entry: T,
    /// 1 if this is the last `in_new=1` row.
    pub is_last_new_entry: T,
    /// Running OR of `in_write` across the shard.
    pub write_seen_prefix: T,

    // ── Key ordering (13) ──
    /// Proves `key < next_key` (StrictIneq + halves + bits).
    pub key_ordering: OrderingRangeChecked<T>,

    // ── LogUp multiplicity witnesses (2) ──
    /// Multiplicity witness for BaseStateEntry bus receive.
    pub read_mult_witness: T,
    /// Multiplicity witness for CoalescedWrite bus receive.
    pub write_mult_witness: T,
    /// Multiplicity for `SSMC_OLD_ENTRY` sends consumed by scheme-owned property chips.
    pub property_anchor_mult: T,
}

/// Compute the width of `StateShardCols` for a given value width.
pub const fn state_shard_width<const W: usize>() -> usize {
    num_cols::<StateShardCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const STATE_SHARD_STANDARD_WIDTH: usize = state_shard_width::<3>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width_is_93() {
        assert_eq!(STATE_SHARD_STANDARD_WIDTH, 116);
    }
}
