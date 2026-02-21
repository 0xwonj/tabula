use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{
    ColumnMeta, CommitmentStrategy, DOMAIN_COL, DOMAIN_LEAF, DOMAIN_TABLE, FieldHasher,
    NativeDigest, SparseMerkleTree,
};
use tabula_core::TableId;
use tabula_core::error::TabulaError;

use crate::air::chips::smt_path::air::{
    SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET, SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
    SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET,
};
use crate::air::chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use crate::witness::BatchWitness;

pub(super) fn validate_smt_path_shapes(
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<(), TabulaError> {
    for (idx, w) in smt_col_paths.iter().enumerate() {
        if w.path_bits.len() != w.siblings.len() {
            return Err(TabulaError::ConsistencyError(format!(
                "smt_col_paths[{idx}] shape mismatch: path_bits={}, siblings={}",
                w.path_bits.len(),
                w.siblings.len()
            )));
        }
    }
    for (idx, w) in smt_table_paths.iter().enumerate() {
        if w.path.path_bits.len() != w.path.siblings.len() {
            return Err(TabulaError::ConsistencyError(format!(
                "smt_table_paths[{idx}] shape mismatch: path_bits={}, siblings={}",
                w.path.path_bits.len(),
                w.path.siblings.len()
            )));
        }
    }
    Ok(())
}

pub(super) fn smt_table_public_values<H>(witness: &BatchWitness<H>) -> Vec<BabyBear>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    smt_table_public_values_from_roots(&witness.old_state_root, &witness.new_state_root)
}

pub(super) fn smt_table_public_values_from_roots(
    old_state_root: &NativeDigest,
    new_state_root: &NativeDigest,
) -> Vec<BabyBear> {
    let mut pvs = vec![BabyBear::ZERO; SMT_TABLE_PATH_NUM_PUBLIC_VALUES];
    pvs[SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET..SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET + 8]
        .copy_from_slice(&old_state_root.0);
    pvs[SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET..SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET + 8]
        .copy_from_slice(&new_state_root.0);
    pvs
}

// ── Public library function ─────────────────────────────────────────────────

/// SMT column-level depth (bits used for column ID within a table).
const COL_DEPTH: usize = 16;
/// SMT table-level depth (bits used for table ID in the global state tree).
const TABLE_DEPTH: usize = 30;

/// Build SMT inclusion-proof witnesses from batch witness metadata.
///
/// Constructs column-level SMT paths (depth 16) per column and
/// table-level SMT paths (depth 30) per table. Validates that
/// the reconstructed roots match the witness's state roots.
pub fn build_smt_paths<H>(
    metas: &[ColumnMeta],
    old_root: &NativeDigest,
    new_root: &NativeDigest,
    hasher: H,
) -> Result<(Vec<SmtPathWitness>, Vec<SmtTablePathWitness>), TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    // Group metas by table.
    let mut by_table: BTreeMap<TableId, Vec<&ColumnMeta>> = BTreeMap::new();
    for meta in metas {
        by_table.entry(meta.table).or_default().push(meta);
    }

    let mut col_paths = Vec::new();
    let mut old_table_roots = BTreeMap::new();
    let mut new_table_roots = BTreeMap::new();
    let mut root_mults = BTreeMap::new();

    for (table, metas_for_table) in &by_table {
        let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);
        let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);

        // Insert all leaves.
        for meta in metas_for_table {
            let tag = commitment_tag(meta.tag);
            let old_leaf = compute_leaf_digest(meta.table.0, meta.col.0, tag, &meta.com_old);
            let new_leaf = compute_leaf_digest(meta.table.0, meta.col.0, tag, &meta.com_new);
            old_tree.insert(meta.col.0 as u64, old_leaf);
            new_tree.insert(meta.col.0 as u64, new_leaf);
        }

        // Generate proofs.
        for meta in metas_for_table {
            let tag = commitment_tag(meta.tag);
            let old_leaf = compute_leaf_digest(meta.table.0, meta.col.0, tag, &meta.com_old);
            let new_leaf = compute_leaf_digest(meta.table.0, meta.col.0, tag, &meta.com_new);

            let old_proof = old_tree.prove(meta.col.0 as u64);
            let new_proof = new_tree.prove(meta.col.0 as u64);

            if old_proof.siblings != new_proof.siblings {
                return Err(TabulaError::ConsistencyError(format!(
                    "old/new sibling vectors differ for column ({:?}, {:?})",
                    meta.table, meta.col
                )));
            }

            col_paths.push(SmtPathWitness {
                table_id: table.0,
                key: meta.col.0 as u32,
                old_leaf,
                new_leaf,
                siblings: old_proof.siblings,
                path_bits: path_bits_from_key(meta.col.0 as u64, COL_DEPTH),
            });
        }

        old_table_roots.insert(*table, old_tree.root());
        new_table_roots.insert(*table, new_tree.root());
        root_mults.insert(*table, metas_for_table.len() as u32);
    }

    // Build table-level SMT.
    let mut old_state_tree = SparseMerkleTree::new(hasher.clone(), TABLE_DEPTH, DOMAIN_TABLE);
    let mut new_state_tree = SparseMerkleTree::new(hasher, TABLE_DEPTH, DOMAIN_TABLE);
    for (&table, &root) in &old_table_roots {
        old_state_tree.insert(table.0 as u64, root);
    }
    for (&table, &root) in &new_table_roots {
        new_state_tree.insert(table.0 as u64, root);
    }

    if old_state_tree.root() != *old_root {
        return Err(TabulaError::ConsistencyError(format!(
            "reconstructed old state root {:?} != witness root {:?}",
            old_state_tree.root(),
            old_root
        )));
    }
    if new_state_tree.root() != *new_root {
        return Err(TabulaError::ConsistencyError(format!(
            "reconstructed new state root {:?} != witness root {:?}",
            new_state_tree.root(),
            new_root
        )));
    }

    let mut table_paths = Vec::new();
    for (&table, &root_mult) in &root_mults {
        let old_leaf = old_table_roots[&table];
        let new_leaf = new_table_roots[&table];
        let old_proof = old_state_tree.prove(table.0 as u64);
        let new_proof = new_state_tree.prove(table.0 as u64);

        if old_proof.siblings != new_proof.siblings {
            return Err(TabulaError::ConsistencyError(format!(
                "old/new sibling vectors differ for table {:?}",
                table
            )));
        }

        table_paths.push(SmtTablePathWitness {
            path: SmtPathWitness {
                table_id: table.0,
                key: table.0,
                old_leaf,
                new_leaf,
                siblings: old_proof.siblings,
                path_bits: path_bits_from_key(table.0 as u64, TABLE_DEPTH),
            },
            root_mult,
        });
    }

    Ok((col_paths, table_paths))
}

fn commitment_tag(strategy: CommitmentStrategy) -> u32 {
    match strategy {
        CommitmentStrategy::Ssmc => 0,
        CommitmentStrategy::Smt => 1,
    }
}

fn compute_leaf_digest(table: u32, col: u16, tag: u32, com: &NativeDigest) -> NativeDigest {
    use crate::air::chips::poseidon::constants::poseidon2_permutation;
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(DOMAIN_LEAF);
    perm_input[1] = BabyBear::new(table);
    perm_input[2] = BabyBear::new(col as u32);
    perm_input[3] = BabyBear::new(tag);
    perm_input[8..16].copy_from_slice(&com.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

fn path_bits_from_key(key: u64, depth: usize) -> Vec<bool> {
    (0..depth).map(|i| ((key >> i) & 1) == 1).collect()
}
