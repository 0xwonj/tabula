//! State root computation: two-level SMT (columns -> tables -> global).
//!
//! Free functions for computing ColumnMeta leaf digests, per-table column
//! roots, and the global state root.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_core::{ColId, TableId};

use crate::column_meta::CommitmentStrategy;
use crate::field::{
    COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_LEAF, DOMAIN_TABLE, NativeDigest, TABLE_STATE_SMT_DEPTH,
};
use crate::hasher::FieldHasher;
use crate::smt::SparseMerkleTree;

/// Compute the ColumnMeta leaf digest.
///
/// Single-permutation compress:
/// `compress([0x10, t, c, tag, 0, 0, 0, 0], com[8])`
///
/// The left half carries the domain tag + identity; the right half is the commitment.
pub fn compute_leaf<H: FieldHasher<F = BabyBear, Digest = NativeDigest>>(
    hasher: &H,
    table: TableId,
    col: ColId,
    tag: CommitmentStrategy,
    commitment: &NativeDigest,
) -> NativeDigest {
    let tag_val: u32 = match tag {
        CommitmentStrategy::Ssmc => 0,
        CommitmentStrategy::Smt => 1,
    };
    let left = NativeDigest([
        BabyBear::new(DOMAIN_LEAF),
        BabyBear::new(table.0),
        BabyBear::new(col.0 as u32),
        BabyBear::new(tag_val),
        BabyBear::ZERO,
        BabyBear::ZERO,
        BabyBear::ZERO,
        BabyBear::ZERO,
    ]);
    hasher.compress(&left, commitment)
}

/// Build column-level SMT from column leaves.
///
/// `SMT_cols(depth=16, domain=DOMAIN_COL)`.
pub fn compute_table_root<H: FieldHasher<F = BabyBear, Digest = NativeDigest>>(
    hasher: &H,
    col_leaves: &BTreeMap<ColId, NativeDigest>,
) -> NativeDigest {
    let mut tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
    for (&col, &leaf) in col_leaves {
        tree.insert(col.0 as u64, leaf);
    }
    tree.root()
}

/// Build table-level SMT from table roots.
///
/// `SMT_tables(depth=30, domain=DOMAIN_TABLE)`.
pub fn compute_state_root<H: FieldHasher<F = BabyBear, Digest = NativeDigest>>(
    hasher: &H,
    table_roots: &BTreeMap<TableId, NativeDigest>,
) -> NativeDigest {
    let mut tree = SparseMerkleTree::new(hasher.clone(), TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE);
    let table_domain_size = 1u64 << TABLE_STATE_SMT_DEPTH;
    for (&table, &root) in table_roots {
        let table_key = table.0 as u64;
        assert!(
            table_key < table_domain_size,
            "table id {} out of range for TABLE_STATE_SMT_DEPTH={} (max allowed: {})",
            table.0,
            TABLE_STATE_SMT_DEPTH,
            table_domain_size - 1
        );
        tree.insert(table_key, root);
    }
    tree.root()
}
