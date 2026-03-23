//! Root-tier SMT path helpers for the current STARK witness path.

use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_commitment::primitives::{
    COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_TABLE, TABLE_STATE_SMT_DEPTH,
};
use tabula_commitment::schemes::smt::SparseMerkleTree;
use tabula_commitment::{
    ColumnRootBinding, FieldHasher, NativeDigest, compute_column_root_binding_leaf,
};
use tabula_core::TableId;
use tabula_core::error::TabulaError;

use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_stark::air::statement::PublicStatement;

pub(crate) fn validate_smt_path_shapes(
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

/// Build a [`PublicStatement`] from batch proof state roots.
pub(crate) fn smt_table_public_statement(
    old_root: &NativeDigest,
    new_root: &NativeDigest,
) -> PublicStatement {
    PublicStatement {
        old_root: *old_root,
        new_root: *new_root,
    }
}

// ── Public library function ─────────────────────────────────────────────────

/// Build SMT inclusion-proof witnesses from batch witness metadata.
///
/// Constructs column-level SMT paths (depth 16) per column and
/// table-level SMT paths (depth 30) per table. Validates that
/// the reconstructed roots match the witness's state roots.
pub(crate) fn build_smt_paths<H>(
    bindings: &[ColumnRootBinding],
    old_root: &NativeDigest,
    new_root: &NativeDigest,
    hasher: H,
) -> Result<(Vec<SmtPathWitness>, Vec<SmtTablePathWitness>), TabulaError>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    // Group metas by table.
    let mut by_table: BTreeMap<TableId, Vec<&ColumnRootBinding>> = BTreeMap::new();
    for binding in bindings {
        by_table.entry(binding.table).or_default().push(binding);
    }

    let mut col_paths = Vec::new();
    let mut old_table_roots = BTreeMap::new();
    let mut new_table_roots = BTreeMap::new();
    let mut root_mults = BTreeMap::new();

    for (table, metas_for_table) in &by_table {
        let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
        let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);

        // Insert all leaves.
        for binding in metas_for_table {
            let old_leaf = compute_column_root_binding_leaf(&hasher, binding, &binding.old_digest);
            let new_leaf = compute_column_root_binding_leaf(&hasher, binding, &binding.new_digest);
            old_tree.insert(binding.col.0 as u64, old_leaf)?;
            new_tree.insert(binding.col.0 as u64, new_leaf)?;
        }

        // Generate proofs only for touched columns.
        // Untouched columns' leaf digests are captured as SMT siblings.
        let mut touched_count = 0u32;
        for binding in metas_for_table {
            if !binding.is_touched {
                continue;
            }
            touched_count += 1;

            let old_leaf = compute_column_root_binding_leaf(&hasher, binding, &binding.old_digest);
            let new_leaf = compute_column_root_binding_leaf(&hasher, binding, &binding.new_digest);

            let old_proof = old_tree.prove(binding.col.0 as u64)?;
            let new_proof = new_tree.prove(binding.col.0 as u64)?;

            col_paths.push(SmtPathWitness {
                table_id: table.0,
                key: binding.col.0 as u32,
                old_leaf,
                new_leaf,
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(binding.col.0 as u64, COL_STATE_SMT_DEPTH),
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
        old_state_tree.insert(table.0 as u64, root)?;
    }
    for (&table, &root) in &new_table_roots {
        new_state_tree.insert(table.0 as u64, root)?;
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
        let old_proof = old_state_tree.prove(table.0 as u64)?;
        let new_proof = new_state_tree.prove(table.0 as u64)?;

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
