//! State root computation: two-level SMT (columns -> tables -> global).
//!
//! Free functions for computing canonical root-binding leaves, per-table
//! column roots, and the global state root.

use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::binding::ColumnRootBinding;
use crate::primitives::FieldHasher;
use crate::primitives::{
    COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_COLUMN_BINDING, DOMAIN_TABLE, NativeDigest,
    TABLE_STATE_SMT_DEPTH,
};
use crate::schemes::smt::SparseMerkleTree;

/// Compute the canonical root-binding prefix digest for one `(table, col, profile)` triple.
pub fn compute_column_root_binding_prefix_digest<
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
>(
    hasher: &H,
    table: TableId,
    col: ColId,
    root_binding_family: tabula_core::RootProfileId,
    column_profile_hash: &tabula_core::Digest,
) -> NativeDigest {
    let mut input = Vec::with_capacity(35);
    input.push(KoalaBear::new(root_binding_family.raw() as u32));
    input.push(KoalaBear::new(table.0));
    input.push(KoalaBear::new(col.0 as u32));
    input.extend(
        column_profile_hash
            .iter()
            .map(|byte| KoalaBear::new(*byte as u32)),
    );
    hasher.hash_domain(DOMAIN_COLUMN_BINDING, &input)
}

/// Compute one canonical root-binding leaf digest.
pub fn compute_column_root_binding_leaf<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    binding: &ColumnRootBinding,
    digest: &crate::NormalizedVerifierDigest,
) -> NativeDigest {
    hasher.compress(&binding.binding_digest, &digest.digest)
}

/// Build column-level SMT from column leaves.
///
/// `SMT_cols(depth=16, domain=DOMAIN_COL)`.
pub fn compute_table_root<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    col_leaves: &BTreeMap<ColId, NativeDigest>,
) -> Result<NativeDigest, TabulaError> {
    let mut tree = SparseMerkleTree::new(hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
    for (&col, &leaf) in col_leaves {
        tree.insert(col.0 as u64, leaf)?;
    }
    Ok(tree.root())
}

/// Build table-level SMT from table roots.
///
/// `SMT_tables(depth=30, domain=DOMAIN_TABLE)`.
pub fn compute_state_root<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    table_roots: &BTreeMap<TableId, NativeDigest>,
) -> Result<NativeDigest, TabulaError> {
    let mut tree = SparseMerkleTree::new(hasher.clone(), TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE);
    for (&table, &root) in table_roots {
        let table_key = table.0 as u64;
        tree.insert(table_key, root).map_err(|err| match err {
            TabulaError::ProofError { .. } => TabulaError::ProofError {
                phase: "commitment",
                detail: format!(
                    "table id {} out of range for TABLE_STATE_SMT_DEPTH={TABLE_STATE_SMT_DEPTH}",
                    table.0
                ),
            },
            other => other,
        })?;
    }
    Ok(tree.root())
}

/// Compute old/new global state roots from canonical column root bindings.
pub fn compute_state_roots_from_bindings<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    bindings: &[ColumnRootBinding],
) -> Result<(NativeDigest, NativeDigest), TabulaError> {
    let mut old_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
    let mut new_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
    let mut seen = BTreeSet::new();

    for binding in bindings {
        if !seen.insert((binding.table, binding.col)) {
            return Err(TabulaError::ProofError {
                phase: "commitment",
                detail: format!(
                    "duplicate ColumnRootBinding entry for table {} column {}",
                    binding.table.0, binding.col.0
                ),
            });
        }
        old_tables.entry(binding.table).or_default().insert(
            binding.col,
            compute_column_root_binding_leaf(hasher, binding, &binding.old_digest),
        );
        new_tables.entry(binding.table).or_default().insert(
            binding.col,
            compute_column_root_binding_leaf(hasher, binding, &binding.new_digest),
        );
    }

    let old_roots: BTreeMap<_, _> = old_tables
        .iter()
        .map(|(table, leaves)| Ok((*table, compute_table_root(hasher, leaves)?)))
        .collect::<Result<_, TabulaError>>()?;
    let new_roots: BTreeMap<_, _> = new_tables
        .iter()
        .map(|(table, leaves)| Ok((*table, compute_table_root(hasher, leaves)?)))
        .collect::<Result<_, TabulaError>>()?;

    Ok((
        compute_state_root(hasher, &old_roots)?,
        compute_state_root(hasher, &new_roots)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PoseidonHasher;
    use crate::primitives::NativeDigest;

    fn digest(n: u32) -> NativeDigest {
        NativeDigest([KoalaBear::new(n); 8])
    }

    fn binding(table: u32, col: u16, old: u32, new: u32) -> ColumnRootBinding {
        ColumnRootBinding {
            table: TableId(table),
            col: ColId(col),
            root_binding_family: tabula_core::RootProfileId::SMT_V1,
            column_profile_hash: [7; 32],
            binding_digest: digest(table + col as u32 + 100),
            old_digest: crate::NormalizedVerifierDigest::new(digest(old)),
            new_digest: crate::NormalizedVerifierDigest::new(digest(new)),
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        }
    }

    #[test]
    fn duplicate_column_root_bindings_are_rejected() {
        let hasher = PoseidonHasher::new();
        let bindings = vec![binding(1, 0, 10, 11), binding(1, 0, 12, 13)];

        let result = compute_state_roots_from_bindings(&hasher, &bindings);

        assert!(result.is_err());
    }

    #[test]
    fn out_of_range_table_ids_return_error() {
        let hasher = PoseidonHasher::new();
        let mut table_roots = BTreeMap::new();
        table_roots.insert(TableId(1u32 << TABLE_STATE_SMT_DEPTH), digest(9));

        let result = compute_state_root(&hasher, &table_roots);

        assert!(result.is_err());
    }
}
