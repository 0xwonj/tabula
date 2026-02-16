//! Column layout for the ColumnMeta AIR.

use crate::air::columns::num_cols;
use crate::air::gadgets::IsZero;

/// Number of BabyBear field elements in a NativeDigest.
pub const DIGEST_WIDTH: usize = 8;

/// Column layout for the ColumnMeta AIR.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
///
/// Upgraded from M6: inverse-based lex ordering replaced with `IsZero` gadgets
/// for sound same-key detection. Range-checked positive ordering deferred to M9
/// (LogUp wiring for RangeCheck bus).
#[repr(C)]
pub struct ColumnMetaCols<T> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,
    /// Commitment strategy tag (0=SSMC, 1=SMT).
    pub tag: T,
    /// Commitment before the batch (8 FE).
    pub com_old: [T; DIGEST_WIDTH],
    /// Commitment after the batch (8 FE).
    pub com_new: [T; DIGEST_WIDTH],
    /// Column was empty before the batch.
    pub is_empty_old: T,
    /// Column is empty after the batch.
    pub is_empty_new: T,
    /// Column was modified in this batch.
    pub is_touched: T,
    // ── Helper witness columns for strict lex ordering (M7 upgrade) ──
    /// IsZero gadget for `(table_id_next - table_id)`.
    pub table_diff_iz: IsZero<T>,
    /// IsZero gadget for `(col_id_next - col_id)` — meaningful only when table IDs match.
    pub col_diff_iz: IsZero<T>,
}

/// Width of the ColumnMeta trace.
pub const COLUMN_META_WIDTH: usize = num_cols::<ColumnMetaCols<u8>, u8>();
