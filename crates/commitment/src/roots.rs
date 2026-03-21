//! State root computation: two-level SMT (columns -> tables -> global).
//!
//! Free functions for computing ColumnMeta leaf digests, per-table column
//! roots, and the global state root.

use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::column::ColumnMeta;
use crate::primitives::FieldHasher;
use crate::primitives::{
    COL_STATE_SMT_DEPTH, DOMAIN_COL, DOMAIN_LEAF, DOMAIN_TABLE, NativeDigest, TABLE_STATE_SMT_DEPTH,
};
use crate::schemes::smt::SparseMerkleTree;

/// Compute the ColumnMeta leaf digest.
///
/// Single-permutation compress:
/// `compress([0x10, t, c, tag, 0, 0, 0, 0], com[8])`
///
/// The left half carries the domain tag + identity; the right half is the commitment.
pub fn compute_column_meta_leaf<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    table: TableId,
    col: ColId,
    tag: u16,
    commitment: &NativeDigest,
) -> NativeDigest {
    let tag_val: u32 = tag as u32;
    let left = NativeDigest([
        KoalaBear::new(DOMAIN_LEAF),
        KoalaBear::new(table.0),
        KoalaBear::new(col.0 as u32),
        KoalaBear::new(tag_val),
        KoalaBear::ZERO,
        KoalaBear::ZERO,
        KoalaBear::ZERO,
        KoalaBear::ZERO,
    ]);
    hasher.compress(&left, commitment)
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

/// Compute old/new global state roots from verifier-visible column metadata.
pub fn compute_state_roots_from_metas<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>>(
    hasher: &H,
    metas: &[ColumnMeta],
) -> Result<(NativeDigest, NativeDigest), TabulaError> {
    let mut old_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
    let mut new_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
    let mut seen = BTreeSet::new();

    for meta in metas {
        if !seen.insert((meta.table, meta.col)) {
            return Err(TabulaError::ProofError {
                phase: "commitment",
                detail: format!(
                    "duplicate ColumnMeta entry for table {} column {}",
                    meta.table.0, meta.col.0
                ),
            });
        }
        old_tables.entry(meta.table).or_default().insert(
            meta.col,
            compute_column_meta_leaf(hasher, meta.table, meta.col, meta.tag, &meta.com_old),
        );
        new_tables.entry(meta.table).or_default().insert(
            meta.col,
            compute_column_meta_leaf(hasher, meta.table, meta.col, meta.tag, &meta.com_new),
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
    use crate::schemes::tags;

    fn digest(n: u32) -> NativeDigest {
        NativeDigest([KoalaBear::new(n); 8])
    }

    fn meta(table: u32, col: u16, old: u32, new: u32) -> ColumnMeta {
        ColumnMeta {
            table: TableId(table),
            col: ColId(col),
            tag: tags::SSMC,
            com_old: digest(old),
            com_new: digest(new),
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        }
    }

    #[test]
    fn duplicate_column_metas_are_rejected() {
        let hasher = PoseidonHasher::new();
        let metas = vec![meta(1, 0, 10, 11), meta(1, 0, 12, 13)];

        let result = compute_state_roots_from_metas(&hasher, &metas);

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
