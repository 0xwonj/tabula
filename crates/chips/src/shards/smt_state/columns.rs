use tabula_gadgets::{IsZero, KeyRangeChecked};
use tabula_stark::air::columns::num_cols;

/// Number of field elements in a native digest.
pub const DIGEST_WIDTH: usize = 8;
/// Row-level SMT depth for committed column state.
pub const SMT_DATA_DEPTH: usize = 32;
/// Low-region switch power (bit 29).
pub const LOW_REGION_SWITCH_POWER: u32 = 1 << 29;
/// Root-level high-region power (bit 31 within the 2-bit high region).
pub const HI_REGION_ROOT_POWER: u32 = 2;

/// Column layout for the SMT state shard AIR.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SmtStateShardCols<T, const W: usize> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    /// Row key for this opened path.
    pub key: KeyRangeChecked<T>,
    /// Old value bound to the base-state bus.
    pub old_val: [T; W],
    /// New value bound to the coalesced-write bus.
    pub new_val: [T; W],
    /// Whether the old value is absent.
    pub old_is_null: T,
    /// Whether the new value is absent.
    pub new_is_null: T,
    /// Multiplicity witness for BaseStateEntry receive.
    pub read_mult_witness: T,
    /// Multiplicity witness for CoalescedWrite receive.
    pub write_mult_witness: T,
    /// 1 if the column commitment before the batch is empty.
    pub column_is_empty_old: T,
    /// 1 if the column commitment after the batch is empty.
    pub column_is_empty_new: T,
    /// 1 if any write touched this column in the batch.
    pub column_is_touched: T,

    /// Column root before the batch.
    pub column_old_root: [T; DIGEST_WIDTH],
    /// Column root after the batch.
    pub column_new_root: [T; DIGEST_WIDTH],

    /// Path direction bit at this level.
    pub path_bit: T,
    /// 1 on the leaf row of a path.
    pub is_leaf: T,
    /// 1 on the root row of a path.
    pub is_root: T,
    /// 1 once the path transitions into the top 2 bits (levels 30-31).
    pub is_hi_region: T,
    /// 1 only on the final root row of the final path in the column.
    pub root_mult_witness: T,

    /// Accumulator for low 30 key bits.
    pub low_key_acc: T,
    /// Power of two for the current low-region row.
    pub low_level_power: T,
    /// Accumulator for high 2 key bits.
    pub hi_key_acc: T,
    /// Power of two for the current high-region row.
    pub hi_level_power: T,
    /// Detects the transition from level 29 to level 30.
    pub switch_level_iz: IsZero<T>,
    /// Detects the final root level (`hi_level_power == 2`).
    pub root_level_iz: IsZero<T>,
    /// Detects whether the next row starts a new path.
    pub next_is_new_path: IsZero<T>,

    /// Poseidon input used to hash the old leaf value (or empty leaf tag).
    pub old_leaf_perm_input: [T; 16],
    /// Poseidon output for the old leaf.
    pub old_leaf_hash: [T; DIGEST_WIDTH],
    /// Poseidon input used to hash the new leaf value (or empty leaf tag).
    pub new_leaf_perm_input: [T; 16],
    /// Poseidon output for the new leaf.
    pub new_leaf_hash: [T; DIGEST_WIDTH],

    /// Current old-tree node digest at this level.
    pub old_node: [T; DIGEST_WIDTH],
    /// Current new-tree node digest at this level.
    pub new_node: [T; DIGEST_WIDTH],
    /// Old-tree sibling digest at this level.
    pub old_sibling: [T; DIGEST_WIDTH],
    /// New-tree sibling digest at this level.
    pub new_sibling: [T; DIGEST_WIDTH],
    /// Poseidon compression input for the old tree.
    pub old_perm_input: [T; 16],
    /// Poseidon compression input for the new tree.
    pub new_perm_input: [T; 16],
    /// Old-tree parent digest for this level.
    pub old_parent: [T; DIGEST_WIDTH],
    /// New-tree parent digest for this level.
    pub new_parent: [T; DIGEST_WIDTH],
}

/// Width of the SMT state shard trace.
pub const fn smt_state_shard_width<const W: usize>() -> usize {
    num_cols::<SmtStateShardCols<u8, W>, u8>()
}
