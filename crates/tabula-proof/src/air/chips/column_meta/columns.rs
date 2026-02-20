//! Column layout for the ColumnMeta AIR.

use crate::air::columns::num_cols;
use crate::air::gadgets::{IsZero, LexOrderingDirection};

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
    /// How many Execution empty-col reads target this `(t,c)` (prover witness).
    /// Free multiplicity: LogUp soundness (2^{-124}) ensures correctness.
    pub empty_read_mult: T,
    // ── Helper witness columns for strict lex ordering (M7 upgrade) ──
    /// IsZero gadget for `(table_id_next - table_id)`.
    pub table_diff_iz: IsZero<T>,
    /// IsZero gadget for `(col_id_next - col_id)` — meaningful only when table IDs match.
    pub col_diff_iz: IsZero<T>,

    // ── Lex ordering direction (M10-A2) ──
    /// Lex ordering direction at segment boundaries (3 cols).
    pub lex: LexOrderingDirection<T>,

    // ── Com_empty verification (M10-B4) ──
    /// Poseidon permutation input for Com_empty: `[0x00, table_id, col_id, 0..]`.
    pub empty_perm_input: [T; 16],
    /// Poseidon permutation output: expected Com_empty digest (8 FE).
    pub empty_perm_output: [T; DIGEST_WIDTH],
    /// 1 if any empty verification needed (is_empty_old OR is_empty_new).
    pub has_empty_check: T,

    // ── Leaf digest (M11 Phase 3) ──
    /// Poseidon input for old leaf: `[0x10, t, c, tag, 0,0,0,0, com_old[8]]`.
    pub leaf_perm_input_old: [T; 16],
    /// Poseidon output: old leaf digest (8 FE).
    pub leaf_digest_old: [T; DIGEST_WIDTH],
    /// Poseidon input for new leaf: `[0x10, t, c, tag, 0,0,0,0, com_new[8]]`.
    pub leaf_perm_input_new: [T; 16],
    /// Poseidon output: new leaf digest (8 FE).
    pub leaf_digest_new: [T; DIGEST_WIDTH],
}

/// Width of the ColumnMeta trace.
pub const COLUMN_META_WIDTH: usize = num_cols::<ColumnMetaCols<u8>, u8>();
