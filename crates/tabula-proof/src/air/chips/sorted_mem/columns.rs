//! Column layout for the GlobalSortedMem AIR.
//!
//! The GlobalSortedMem table is the main memory consistency table.
//! Rows are sorted by `(table_id, col_id, r, tau)` with segments
//! delimited by `(table_id, col_id)` changes.
//!
//! Each segment begins with an init row (`is_init=1, tau=0`),
//! followed by access rows in timestamp order.

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, LimbHalves, StrictIneq, U64Limbs};

/// Column layout for the GlobalSortedMem AIR.
///
/// Generic over `W` (value width in field elements).
/// Standard width: W=3 (U64/I64). Narrow: W=1 (Bool). Wide: W=8 (Digest).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct GlobalSortedMemCols<T, const W: usize> {
    // ── Identity ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    // ── Cell address ──
    /// Row key (u64 as 3 BabyBear limbs).
    pub r: U64Limbs<T>,

    // ── Timestamp ──
    /// Timestamp (u64 as 3 BabyBear limbs). tau = clk + 1 for access rows; tau = 0 for init.
    pub tau: U64Limbs<T>,
    /// Init row flag (1 = init, 0 = access).
    pub is_init: T,
    /// Write flag (1 = write, 0 = read). Init rows have is_write = 0.
    pub is_write: T,

    // ── Value (Tier 2 encoding) ──
    /// Value field elements.
    pub val: [T; W],
    /// Null flag for the value.
    pub val_is_null: T,

    // ── Running memory ──
    /// Running memory state (carries forward the last written value for this key).
    pub mem: [T; W],
    /// Running memory null flag.
    pub mem_is_null: T,

    // ── Write-set extraction ──
    /// 1 if this is the last row for the current `(t, c, r)` key.
    pub is_last_for_key: T,
    /// 1 if any write has occurred for the current `(t, c, r)` key.
    pub has_written: T,

    // ── Segment metadata (for SortedMemMeta bus) ──
    /// 1 if this is the first row of a `(t,c)` segment.
    pub is_first_of_segment: T,
    /// For first-of-segment rows: 1 if the column was empty in the old state.
    pub meta_is_empty_old: T,

    // ── Range-check half-decomposition (for RangeCheck bus) ──
    /// Half-decomposition of r.limb0 (15+15 bits).
    pub r_l0_halves: LimbHalves<T>,
    /// Half-decomposition of r.limb1 (15+15 bits).
    pub r_l1_halves: LimbHalves<T>,
    /// Half-decomposition of tau.limb0 (15+15 bits).
    pub tau_l0_halves: LimbHalves<T>,
    /// Half-decomposition of tau.limb1 (15+15 bits).
    pub tau_l1_halves: LimbHalves<T>,

    // ── Ordering helpers ──
    /// 1 if `(table_id, col_id)` changes from this row to the next.
    pub tc_changed: T,
    /// 1 if the row key `r` changes from this row to the next (within same `(t,c)`).
    pub r_changed: T,

    // ── Inverse helpers for same-key detection ──
    /// IsZero for `(next.table_id - table_id)`.
    pub table_diff_iz: IsZero<T>,
    /// IsZero for `(next.col_id - col_id)`.
    pub col_diff_iz: IsZero<T>,
    /// IsZero for combined row key diff (used for r_changed detection).
    /// We compute `r_diff = (next.r0 - r0) + (next.r1 - r1) * alpha + (next.r2 - r2) * alpha^2`
    /// using a random-challenge-free approach: just check all 3 limbs individually.
    /// `r_diff_iz` tracks whether ALL limbs are equal (via product).
    pub r_diff_iz: IsZero<T>,

    // ── Shared ordering gadget (r or tau strict inequality) ──
    /// Proves either `r_next > r` (when key changes) or `tau_next > tau` (same key).
    pub ordering: StrictIneq<T>,
}

/// Compute the width of GlobalSortedMemCols for a given value width.
pub const fn sorted_mem_width<const W: usize>() -> usize {
    num_cols::<GlobalSortedMemCols<u8, W>, u8>()
}

/// Width for Standard value width (W=3).
pub const SORTED_MEM_STANDARD_WIDTH: usize = sorted_mem_width::<3>();
