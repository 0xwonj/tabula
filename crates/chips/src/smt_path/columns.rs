//! Column layout for SmtPath AIR chips.

use tabula_gadgets::IsZero;
use tabula_stark::air::columns::num_cols;

/// Number of BabyBear field elements in a NativeDigest.
pub const DIGEST_WIDTH: usize = 8;

/// Column layout shared by SmtColPathChip and SmtTablePathChip.
///
/// Each row represents one level of a Merkle path traversal.
/// Paths are laid out contiguously: rows for path_0 (leaf→root), then path_1, etc.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct SmtPathCols<T> {
    // ── Control (4) ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Which branch to take at this level (0=left, 1=right).
    pub path_bit: T,
    /// 1 if this is the leaf level (level == 0).
    pub is_leaf: T,
    /// 1 if this is the root level (last level of path).
    pub is_root: T,

    // ── Identity (2) ──
    /// Table identifier (constant within a path).
    pub bind_table_id: T,
    /// Key being proven (col_id for col-level, table_id for table-level).
    pub bind_key: T,

    // ── Sibling (8) ──
    /// Sibling node at this level.
    pub sibling: [T; DIGEST_WIDTH],

    // ── Old tree (16) ──
    /// Old node at this level.
    pub old_node: [T; DIGEST_WIDTH],
    /// Old parent (output of Poseidon compress at this level).
    pub old_parent: [T; DIGEST_WIDTH],

    // ── New tree (16) ──
    /// New node at this level.
    pub new_node: [T; DIGEST_WIDTH],
    /// New parent (output of Poseidon compress at this level).
    pub new_parent: [T; DIGEST_WIDTH],

    // ── Poseidon mux witnesses (32) ──
    // path_bit selects left/right ordering for Poseidon compress.
    // Constrained: left[i] = (1-bit)*node[i] + bit*sib[i]
    //              right[i] = bit*node[i] + (1-bit)*sib[i]
    /// Poseidon input for old tree: `[left[8], right[8]]`.
    pub old_perm_input: [T; 16],
    /// Poseidon input for new tree: `[left[8], right[8]]`.
    pub new_perm_input: [T; 16],

    // ── Key reconstruction (2) ──
    /// Running key accumulator: `Σ path_bit_i × 2^i`.
    pub key_acc: T,
    /// Running power of 2: `2^level`.
    pub level_power: T,

    // ── Path boundary detection (2) ──
    /// IsZero on `(next.path_id - local.path_id)` — encoded as bind_key difference.
    /// When `is_zero = 1`, the next row belongs to a different path.
    pub next_is_new_path: IsZero<T>,
}

/// Width of SmtColPathChip (no extra columns).
pub const SMT_COL_PATH_WIDTH: usize = num_cols::<SmtPathCols<u8>, u8>();

/// Column layout for SmtTablePathChip — extends SmtPathCols with root multiplicity.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct SmtTablePathCols<T> {
    /// Shared path columns.
    pub base: SmtPathCols<T>,
    /// Multiplicity witness for C16 receive at leaf level.
    /// Set to N (number of columns for this table) for the leaf row.
    /// LogUp soundness ensures correct value.
    pub root_mult_witness: T,
}

/// Width of SmtTablePathChip (base + 1).
pub const SMT_TABLE_PATH_WIDTH: usize = num_cols::<SmtTablePathCols<u8>, u8>();
