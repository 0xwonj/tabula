use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_commitment::{
    COL_STATE_SMT_DEPTH, ColumnMeta, DOMAIN_COL, DOMAIN_TABLE, FieldHasher, NativeDigest,
    SparseMerkleTree, TABLE_STATE_SMT_DEPTH, compute_leaf,
};
use tabula_core::TableId;
use tabula_core::error::TabulaError;

use crate::witness::BatchWitness;
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_stark::air::statement::PublicStatement;

pub(super) fn validate_smt_path_shapes(
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<(), TabulaError> {
    for (idx, w) in smt_col_paths.iter().enumerate() {
        if w.path_bits.len() != w.old_siblings.len() {
            return Err(TabulaError::ProofError {
                phase: "smt",
                detail: format!(
                    "smt_col_paths[{idx}] shape mismatch: path_bits={}, old_siblings={}",
                    w.path_bits.len(),
                    w.old_siblings.len()
                ),
            });
        }
        if w.path_bits.len() != w.new_siblings.len() {
            return Err(TabulaError::ProofError {
                phase: "smt",
                detail: format!(
                    "smt_col_paths[{idx}] shape mismatch: path_bits={}, new_siblings={}",
                    w.path_bits.len(),
                    w.new_siblings.len()
                ),
            });
        }
    }
    for (idx, w) in smt_table_paths.iter().enumerate() {
        if w.path.path_bits.len() != w.path.old_siblings.len() {
            return Err(TabulaError::ProofError {
                phase: "smt",
                detail: format!(
                    "smt_table_paths[{idx}] shape mismatch: path_bits={}, old_siblings={}",
                    w.path.path_bits.len(),
                    w.path.old_siblings.len()
                ),
            });
        }
        if w.path.path_bits.len() != w.path.new_siblings.len() {
            return Err(TabulaError::ProofError {
                phase: "smt",
                detail: format!(
                    "smt_table_paths[{idx}] shape mismatch: path_bits={}, new_siblings={}",
                    w.path.path_bits.len(),
                    w.path.new_siblings.len()
                ),
            });
        }
    }
    Ok(())
}

/// Build a [`PublicStatement`] from batch witness state roots.
pub(super) fn smt_table_public_statement<H>(witness: &BatchWitness<H>) -> PublicStatement
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    }
}

// ── Public library function ─────────────────────────────────────────────────

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
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
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
        let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
        let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);

        // Insert all leaves.
        for meta in metas_for_table {
            let old_leaf = compute_leaf(&hasher, meta.table, meta.col, meta.tag, &meta.com_old);
            let new_leaf = compute_leaf(&hasher, meta.table, meta.col, meta.tag, &meta.com_new);
            old_tree.insert(meta.col.0 as u64, old_leaf);
            new_tree.insert(meta.col.0 as u64, new_leaf);
        }

        // Generate proofs only for touched columns.
        // Untouched columns' leaf digests are captured as SMT siblings.
        let mut touched_count = 0u32;
        for meta in metas_for_table {
            if !meta.is_touched {
                continue;
            }
            touched_count += 1;

            let old_leaf = compute_leaf(&hasher, meta.table, meta.col, meta.tag, &meta.com_old);
            let new_leaf = compute_leaf(&hasher, meta.table, meta.col, meta.tag, &meta.com_new);

            let old_proof = old_tree.prove(meta.col.0 as u64);
            let new_proof = new_tree.prove(meta.col.0 as u64);

            col_paths.push(SmtPathWitness {
                table_id: table.0,
                key: meta.col.0 as u32,
                old_leaf,
                new_leaf,
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(meta.col.0 as u64, COL_STATE_SMT_DEPTH),
            });
        }

        old_table_roots.insert(*table, old_tree.root());
        new_table_roots.insert(*table, new_tree.root());
        root_mults.insert(*table, touched_count);
    }

    // Build table-level SMT.
    let mut old_state_tree =
        SparseMerkleTree::new(hasher.clone(), TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE);
    let mut new_state_tree = SparseMerkleTree::new(hasher, TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE);
    for (&table, &root) in &old_table_roots {
        old_state_tree.insert(table.0 as u64, root);
    }
    for (&table, &root) in &new_table_roots {
        new_state_tree.insert(table.0 as u64, root);
    }

    if old_state_tree.root() != *old_root {
        return Err(TabulaError::ProofError {
            phase: "smt",
            detail: format!(
                "reconstructed old state root {:?} != witness root {:?}",
                old_state_tree.root(),
                old_root
            ),
        });
    }
    if new_state_tree.root() != *new_root {
        return Err(TabulaError::ProofError {
            phase: "smt",
            detail: format!(
                "reconstructed new state root {:?} != witness root {:?}",
                new_state_tree.root(),
                new_root
            ),
        });
    }

    let mut table_paths = Vec::new();
    for (&table, &root_mult) in &root_mults {
        let old_leaf = old_table_roots[&table];
        let new_leaf = new_table_roots[&table];
        let old_proof = old_state_tree.prove(table.0 as u64);
        let new_proof = new_state_tree.prove(table.0 as u64);

        table_paths.push(SmtTablePathWitness {
            path: SmtPathWitness {
                table_id: table.0,
                key: table.0,
                old_leaf,
                new_leaf,
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(table.0 as u64, TABLE_STATE_SMT_DEPTH),
            },
            root_mult,
        });
    }

    Ok((col_paths, table_paths))
}

fn path_bits_from_key(key: u64, depth: usize) -> Vec<bool> {
    (0..depth).map(|i| ((key >> i) & 1) == 1).collect()
}
