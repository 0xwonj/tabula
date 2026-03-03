//! Column layout for the InterTxOrder AIR.
//!
//! Mediates between Execution and StateColumn for conflicting batches.
//! Rows sorted by `(t, c, key, tx_index)` within per-`(t,c)` segments.
//! Each key has exactly 1 init row + N access rows (one per tx that touched it).
//!
//! Column budget: 56 (W=3).

use crate::air::columns::num_cols;
use crate::gadgets::{
    IsZero, KeyRangeChecked, LexOrderingDirection, OrderingRangeChecked, SameKeyDetection,
};

/// Column layout for the InterTxOrder AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct InterTxOrderCols<T, const W: usize> {
    // ── Identity (3) ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    // ── Key (11) ── KeyRangeChecked gadget
    /// Row key (u64 limbs + half-decomposition for range checks).
    pub key: KeyRangeChecked<T>,

    // ── Tx ordering (2) ──
    /// Transaction index within the batch.
    pub tx_index: T,
    /// `next.tx_index - tx_index - 1` for same-key rows (range-checked).
    pub tx_diff: T,

    // ── Row type + access flags (3) ──
    /// 1 if this is an init row (base state seed); 0 if tx access row.
    pub is_init: T,
    /// 1 if this tx read the key.
    pub has_read: T,
    /// 1 if this tx wrote the key.
    pub has_write: T,

    // ── Input value: what this tx sees (W+1 = 4) ──
    /// Input value limbs (for init: base state value; for access: prev output).
    pub input_val: [T; W],
    /// Input is-null flag.
    pub input_is_null: T,

    // ── Output value: what this tx produces (W+1 = 4) ──
    /// Output value limbs (for init: same as input; for write: written value).
    pub output_val: [T; W],
    /// Output is-null flag.
    pub output_is_null: T,

    // ── Chain tracking (2) ──
    /// 1 if this is the last row before the key changes (or end of real rows).
    pub is_last_for_key: T,
    /// Monotone flag within a key chain: 1 once any write has occurred.
    pub has_ever_written: T,

    // ── Same-key detection (11) ──
    /// Same-(t,c) detection (5 cols: IsZero×2 + tc_changed).
    pub same_tc: SameKeyDetection<T>,
    /// IsZero for `next.key.limb0 - local.key.limb0`.
    pub r_limb0_iz: IsZero<T>,
    /// IsZero for `next.key.limb1 - local.key.limb1`.
    pub r_limb1_iz: IsZero<T>,
    /// IsZero for `next.key.limb2 - local.key.limb2`.
    pub r_limb2_iz: IsZero<T>,

    // ── Key ordering when keys differ (13) ──
    /// Proves `key < next_key` within same segment when keys differ.
    pub key_ordering: OrderingRangeChecked<T>,

    // ── Lex ordering at segment boundary (3) ──
    /// Lex ordering direction at `(t,c)` segment boundaries.
    pub lex_dir: LexOrderingDirection<T>,
}

/// Compute the width of InterTxOrderCols for a given value width.
pub const fn inter_tx_order_width<const W: usize>() -> usize {
    num_cols::<InterTxOrderCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const INTER_TX_ORDER_STANDARD_WIDTH: usize = inter_tx_order_width::<3>();
