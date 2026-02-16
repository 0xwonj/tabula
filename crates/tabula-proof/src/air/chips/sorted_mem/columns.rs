//! Column layout for the GlobalSortedMem AIR.
//!
//! The GlobalSortedMem table is the main memory consistency table.
//! Rows are sorted by `(table_id, col_id, r, tau)` with segments
//! delimited by `(table_id, col_id)` changes.
//!
//! Each segment begins with an init row (`is_init=1, tau=0`),
//! followed by access rows in timestamp order.

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, StrictIneq, U64Limbs};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width() {
        // is_real(1) + table_id(1) + col_id(1) + r(3) + tau(3) + is_init(1) + is_write(1)
        // + val(3) + val_is_null(1) + mem(3) + mem_is_null(1)
        // + is_last_for_key(1) + has_written(1) + tc_changed(1) + r_changed(1)
        // + table_diff_iz(2) + col_diff_iz(2) + r_diff_iz(2) + ordering(3)
        // = 1+1+1+3+3+1+1+3+1+3+1+1+1+1+1+2+2+2+3 = 32
        assert_eq!(SORTED_MEM_STANDARD_WIDTH, 32);
    }
}
