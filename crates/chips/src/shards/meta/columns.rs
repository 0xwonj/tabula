//! Column layout for the MetaShard AIR.
//!
//! Per-column version of `ColumnMetaCols`. Removes ordering-related columns
//! (IsZero×2, LexOrderingDirection) since each shard handles a single `(t, c)`.
//!
//! The `tag` field is NOT a trace column — it is a constructor parameter on
//! [`MetaShardChip`](super::air::MetaShardChip). This eliminates the boolean
//! constraint on tag and allows an unlimited number of commitment schemes.

use tabula_stark::air::columns::num_cols;

/// Number of KoalaBear field elements in a NativeDigest.
pub const DIGEST_WIDTH: usize = 8;

/// Column layout for the MetaShard AIR.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
///
/// Compared to the previous version (97 cols), saves 1 column by moving
/// `tag` from a trace column to a chip constructor parameter.
#[repr(C)]
pub struct MetaShardCols<T> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier (constant across all real rows).
    pub table_id: T,
    /// Column identifier (constant across all real rows).
    pub col_id: T,
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
    /// Whether a scheme-owned state chip is expected to provide C6 commitments.
    pub has_commitment_proof: T,
    /// How many Execution empty-col reads target this `(t,c)` (prover witness).
    pub empty_read_mult: T,

    // ── Com_empty verification ──
    /// Poseidon permutation input for Com_empty: `[0x00, table_id, col_id, 0..]`.
    pub empty_perm_input: [T; 16],
    /// Poseidon permutation output: expected Com_empty digest (8 FE).
    pub empty_perm_output: [T; DIGEST_WIDTH],
    /// 1 if any empty verification needed (is_empty_old OR is_empty_new).
    pub has_empty_check: T,

    // ── Leaf digest ──
    /// Poseidon input for old leaf: `[0x10, t, c, scheme_tag, 0,0,0,0, com_old[8]]`.
    pub leaf_perm_input_old: [T; 16],
    /// Poseidon output: old leaf digest (8 FE).
    pub leaf_digest_old: [T; DIGEST_WIDTH],
    /// Poseidon input for new leaf: `[0x10, t, c, scheme_tag, 0,0,0,0, com_new[8]]`.
    pub leaf_perm_input_new: [T; 16],
    /// Poseidon output: new leaf digest (8 FE).
    pub leaf_digest_new: [T; DIGEST_WIDTH],
}

/// Width of the MetaShard trace.
pub const META_SHARD_WIDTH: usize = num_cols::<MetaShardCols<u8>, u8>();
