//! Column layout for the MemoryShard AIR.
//!
//! Per-column version of `InterTxOrderCols`, operating on a single `(t, c)`.
//! Rows sorted by `(key, tx_index)`. Each key has exactly 1 init row + N
//! access rows (one per tx that touched it).
//!
//! Compared to the global `InterTxOrderCols`:
//! - Removed `SameKeyDetection` (5 cols): no segment boundaries
//! - Removed `LexOrderingDirection` (3 cols): no cross-segment ordering
//!
//! Column budget: 48 (W=3).

use tabula_gadgets::{IsZero, KeyRangeChecked, OrderingRangeChecked};
use tabula_stark::air::columns::num_cols;

/// Column layout for the MemoryShard AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct MemoryShardCols<T, const W: usize> {
    // ── Identity (3) ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier (constant across all rows).
    pub table_id: T,
    /// Column identifier (constant across all rows).
    pub col_id: T,

    // ── Key (11) ── KeyRangeChecked gadget
    /// Row key (u64 limbs + half-decomposition for range checks).
    pub key: KeyRangeChecked<T>,

    // ── Tx ordering (2) ──
    /// Transaction index within the batch.
    pub tx_index: T,
    /// `next.tx_index - tx_index - 1` for same-key consecutive access rows.
    pub tx_diff: T,

    // ── Row type + access flags (3) ──
    /// 1 if this is an init row (base state seed); 0 if tx access row.
    pub is_init: T,
    /// 1 if this tx read the key.
    pub has_read: T,
    /// 1 if this tx wrote the key.
    pub has_write: T,

    // ── Input value: what this tx sees (W+1) ──
    /// Input value limbs (for init: base state value; for access: prev output).
    pub input_val: [T; W],
    /// Input is-null flag.
    pub input_is_null: T,

    // ── Output value: what this tx produces (W+1) ──
    /// Output value limbs (for init: same as input; for write: written value).
    pub output_val: [T; W],
    /// Output is-null flag.
    pub output_is_null: T,

    // ── Chain tracking (2) ──
    /// 1 if this is the last row before the key changes (or end of real rows).
    pub is_last_for_key: T,
    /// Monotone flag within a key chain: 1 once any write has occurred.
    pub has_ever_written: T,

    // ── Key-change detection via IsZero (6) ──
    /// IsZero for `next.key.limb0 - local.key.limb0`.
    pub r_limb0_iz: IsZero<T>,
    /// IsZero for `next.key.limb1 - local.key.limb1`.
    pub r_limb1_iz: IsZero<T>,
    /// IsZero for `next.key.limb2 - local.key.limb2`.
    pub r_limb2_iz: IsZero<T>,

    // ── Key ordering when keys differ (13) ──
    /// Proves `key < next_key` when keys differ.
    pub key_ordering: OrderingRangeChecked<T>,
}

/// Compute the width of `MemoryShardCols` for a given value width.
pub const fn memory_shard_width<const W: usize>() -> usize {
    num_cols::<MemoryShardCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const MEMORY_SHARD_STANDARD_WIDTH: usize = memory_shard_width::<3>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width_is_48() {
        assert_eq!(MEMORY_SHARD_STANDARD_WIDTH, 48);
    }

    #[test]
    fn narrow_width() {
        assert_eq!(memory_shard_width::<1>(), 44);
    }

    #[test]
    fn wide_width() {
        assert_eq!(memory_shard_width::<8>(), 58);
    }
}
