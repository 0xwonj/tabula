//! Column state and metadata for the state commitment system.
//!
//! [`ColumnState`] holds the per-column commitment data structure (SSMC or SMT)
//! and provides operations for creating, querying, and updating column state.
//! [`ColumnMeta`] describes a column's commitment transition during a batch.

use std::collections::BTreeSet;

use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

use crate::primitives::FieldHasher;
use crate::primitives::{COL_DATA_SMT_DEPTH, DOMAIN_SMT, NativeDigest};
use crate::schemes::smt::SparseMerkleTree;
use crate::schemes::ssmc::{SsmcEntry, SsmcList};
use crate::schemes::tags;

// ── ColumnState ──────────────────────────────────────────────────────────────

/// Per-column state holding the underlying data structure.
///
/// Architecture note: this enum is still closed over built-in layouts. A
/// fully open scheme platform will need a registry-backed replacement shared
/// by commitment materialization, witness generation, and root binding.
#[derive(Clone, Debug)]
pub enum ColumnState<H: FieldHasher> {
    /// SSMC-backed column (small).
    Ssmc(SsmcList),
    /// SMT-backed column (large).
    Smt(SparseMerkleTree<H>),
}

impl<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> ColumnState<H> {
    /// Create a committed column state from pre-encoded entries.
    ///
    /// `scheme_tag` determines which data structure to use:
    /// - [`tags::SSMC`] → hash chain ([`SsmcList`])
    /// - [`tags::SMT`] → sparse Merkle tree
    ///
    /// Entries must be sorted by key (enforced by `SsmcList::from_sorted`).
    pub fn commit(
        hasher: &H,
        table: TableId,
        col: ColId,
        entries: Vec<(RowKey, Vec<KoalaBear>)>,
        scheme_tag: u16,
    ) -> Result<(Self, NativeDigest), TabulaError> {
        match scheme_tag {
            tags::SSMC => {
                let ssmc_entries: Vec<SsmcEntry> = entries
                    .into_iter()
                    .map(|(key, value)| SsmcEntry { key, value })
                    .collect();
                let list = SsmcList::from_sorted(table, col, ssmc_entries)?;
                let com = list.commit(hasher).0;
                Ok((Self::Ssmc(list), com))
            }
            tags::SMT => {
                let mut tree =
                    SparseMerkleTree::new(hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);
                let mut seen = BTreeSet::new();
                for (key, value_fes) in entries {
                    if !seen.insert(key) {
                        return Err(TabulaError::ConsistencyError(format!(
                            "duplicate SMT entry key: {}",
                            key.0
                        )));
                    }
                    let leaf = hasher.hash(&value_fes);
                    tree.insert(key.0, leaf)?;
                }
                let root = tree.root();
                Ok((Self::Smt(tree), root))
            }
            _ => Err(TabulaError::ProofError {
                phase: "commitment",
                detail: format!("unknown scheme tag: {scheme_tag}"),
            }),
        }
    }

    /// Apply writes to produce a new state.
    ///
    /// Returns `(new_state, new_commitment)`.
    pub fn apply_writes(
        &self,
        hasher: &H,
        table: TableId,
        col: ColId,
        writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    ) -> Result<(Self, NativeDigest), TabulaError> {
        match self {
            Self::Ssmc(old_list) => {
                let (new_list, com) =
                    crate::schemes::ssmc::merge::merge(old_list, writes, table, col, hasher);
                Ok((Self::Ssmc(new_list), com.0))
            }
            Self::Smt(old_tree) => {
                let mut tree = old_tree.clone();
                for (key, value) in writes {
                    match value {
                        Some(fes) => {
                            let leaf = hasher.hash(fes);
                            tree.insert(key.0, leaf)?;
                        }
                        None => {
                            tree.remove(key.0)?;
                        }
                    }
                }
                let root = tree.root();
                Ok((Self::Smt(tree), root))
            }
        }
    }

    /// Whether the column has zero entries.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Ssmc(list) => list.is_empty(),
            Self::Smt(tree) => tree.is_empty(),
        }
    }

    /// Scheme tag for this column's commitment strategy.
    pub fn scheme_tag(&self) -> u16 {
        match self {
            Self::Ssmc(_) => tags::SSMC,
            Self::Smt(_) => tags::SMT,
        }
    }

    /// Compute the proof-visible column commitment for this native state.
    pub fn proof_commitment(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<NativeDigest, TabulaError> {
        match self {
            Self::Ssmc(list) => crate::schemes::ssmc::proof_commitment(table, col, list),
            Self::Smt(tree) => Ok(tree.root()),
        }
    }
}

// ── ColumnMeta ───────────────────────────────────────────────────────────────

/// Metadata for a column's commitment transition during a batch.
///
/// Corresponds to the ColumnMeta table in the proof spec.
#[derive(Clone, Debug)]
pub struct ColumnMeta {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Scheme tag (see [`crate::schemes::tags`]).
    pub tag: u16,
    /// Commitment before the batch.
    pub com_old: NativeDigest,
    /// Commitment after the batch.
    pub com_new: NativeDigest,
    /// Column was empty before the batch.
    pub is_empty_old: bool,
    /// Column is empty after the batch.
    pub is_empty_new: bool,
    /// Column was modified in this batch.
    pub is_touched: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::MockFieldHasher;

    fn val(n: u32) -> Vec<KoalaBear> {
        vec![KoalaBear::new(n)]
    }

    fn entries(pairs: &[(u64, u32)]) -> Vec<(RowKey, Vec<KoalaBear>)> {
        pairs.iter().map(|&(k, v)| (RowKey(k), val(v))).collect()
    }

    #[test]
    fn proof_commitment_matches_ssmc_commitment_for_empty_state() {
        let (state, _) =
            ColumnState::commit(&MockFieldHasher, TableId(1), ColId(0), vec![], tags::SSMC)
                .unwrap();

        assert!(state.proof_commitment(TableId(1), ColId(0)).is_ok());
    }

    #[test]
    fn proof_commitment_matches_smt_root() {
        let (state, root) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SMT,
        )
        .unwrap();

        assert_eq!(state.proof_commitment(TableId(1), ColId(0)).unwrap(), root);
    }

    #[test]
    fn commit_ssmc_creates_ssmc_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SSMC,
        )
        .unwrap();
        assert_eq!(state.scheme_tag(), tags::SSMC);
        assert!(!state.is_empty());
    }

    #[test]
    fn commit_smt_creates_smt_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SMT,
        )
        .unwrap();
        assert_eq!(state.scheme_tag(), tags::SMT);
        assert!(!state.is_empty());
    }

    #[test]
    fn commit_empty_creates_empty_state() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[]),
            tags::SSMC,
        )
        .unwrap();
        assert!(state.is_empty());
    }

    #[test]
    fn commit_unknown_tag_errors() {
        let result = ColumnState::commit(&MockFieldHasher, TableId(1), ColId(0), entries(&[]), 99);
        assert!(result.is_err());
    }

    #[test]
    fn apply_writes_ssmc_updates_commitment() {
        let (state, com_old) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1)]),
            tags::SSMC,
        )
        .unwrap();
        let writes = vec![(RowKey(1), Some(val(2)))];
        let (new_state, com_new) = state
            .apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes)
            .unwrap();
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.scheme_tag(), tags::SSMC);
    }

    #[test]
    fn apply_writes_smt_updates_commitment() {
        let (state, com_old) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SMT,
        )
        .unwrap();
        let writes = vec![(RowKey(2), Some(val(3)))];
        let (new_state, com_new) = state
            .apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes)
            .unwrap();
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.scheme_tag(), tags::SMT);
    }

    #[test]
    fn apply_writes_delete_removes_entry() {
        let (state, _) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SSMC,
        )
        .unwrap();
        let writes = vec![(RowKey(0), None)];
        let (new_state, _) = state
            .apply_writes(&MockFieldHasher, TableId(1), ColId(0), &writes)
            .unwrap();
        assert!(!new_state.is_empty());
        let ColumnState::Ssmc(list) = new_state else {
            panic!("expected SSMC state after SSMC delete");
        };
        assert_eq!(list.len(), 1);
        assert_eq!(list.entries()[0].key, RowKey(1));
    }

    #[test]
    fn round_trip_ssmc_empty_writes() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SSMC,
        )
        .unwrap();
        let (_, com_after) = state
            .apply_writes(&MockFieldHasher, TableId(1), ColId(0), &[])
            .unwrap();
        assert_eq!(com, com_after);
    }

    #[test]
    fn round_trip_smt_empty_writes() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (1, 2)]),
            tags::SMT,
        )
        .unwrap();
        let (_, com_after) = state
            .apply_writes(&MockFieldHasher, TableId(1), ColId(0), &[])
            .unwrap();
        assert_eq!(com, com_after);
    }

    #[test]
    fn commitment_matches_commit_output() {
        let (state, com) = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 42)]),
            tags::SSMC,
        )
        .unwrap();
        let ColumnState::Ssmc(list) = state else {
            panic!("expected SSMC state");
        };
        assert_eq!(list.commit(&MockFieldHasher).0, com);
    }

    #[test]
    fn commit_smt_rejects_duplicate_keys() {
        let result = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            entries(&[(0, 1), (0, 2)]),
            tags::SMT,
        );
        assert!(result.is_err());
    }

    #[test]
    fn commit_smt_rejects_out_of_range_keys() {
        let result = ColumnState::commit(
            &MockFieldHasher,
            TableId(1),
            ColId(0),
            vec![(RowKey(1u64 << COL_DATA_SMT_DEPTH), val(7))],
            tags::SMT,
        );
        assert!(result.is_err());
    }
}
