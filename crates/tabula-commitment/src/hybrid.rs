//! Hybrid state commitment: auto-selects SSMC or SMT per column.
//!
//! Dispatches between SSMC (hash chain, for small columns) and SMT (Merkle tree,
//! for large columns) based on a configurable threshold. Provides primitives for:
//! - Per-column commitment and update
//! - Two-level state root computation (`SMT_cols` → `SMT_tables`)
//! - ColumnMeta leaf construction

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_core::{ColId, RowKey, TableId};

use crate::field::{DOMAIN_COL, DOMAIN_LEAF, DOMAIN_SMT, DOMAIN_TABLE, NativeDigest};
use crate::hasher::FieldHasher;
use crate::smt::SparseMerkleTree;
use crate::ssmc::{MergeTrace, SsmcEntry, SsmcList};

// ── Constants ──────────────────────────────────────────────────────────────

/// Depth for per-column data SMTs (row-level key space, 2^32 keys).
const COL_DATA_SMT_DEPTH: usize = 32;

/// Depth for the column-level state SMT (`SMT_cols`).
const COL_STATE_SMT_DEPTH: usize = 16;

/// Depth for the table-level state SMT (`SMT_tables`).
const TABLE_STATE_SMT_DEPTH: usize = 32;

// ── Types ──────────────────────────────────────────────────────────────────

/// Strategy used for a column's commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentStrategy {
    /// Small Sparse Map Commitment (hash chain). Used when entry count ≤ threshold.
    Ssmc,
    /// Sparse Merkle Tree. Used when entry count > threshold.
    Smt,
}

/// Per-column state holding the underlying data structure.
#[derive(Clone, Debug)]
pub enum ColumnState<H: FieldHasher> {
    /// SSMC-backed column (small).
    Ssmc(SsmcList),
    /// SMT-backed column (large).
    Smt(SparseMerkleTree<H>),
}

impl<H: FieldHasher> ColumnState<H> {
    /// Whether the column has zero entries.
    pub fn is_empty(&self) -> bool {
        match self {
            ColumnState::Ssmc(list) => list.is_empty(),
            ColumnState::Smt(tree) => tree.is_empty(),
        }
    }

    /// Which strategy this column uses.
    pub fn strategy(&self) -> CommitmentStrategy {
        match self {
            ColumnState::Ssmc(_) => CommitmentStrategy::Ssmc,
            ColumnState::Smt(_) => CommitmentStrategy::Smt,
        }
    }
}

/// Metadata for a column's commitment transition during a batch.
///
/// Corresponds to the ColumnMeta table in the proof spec.
#[derive(Clone, Debug)]
pub struct ColumnMeta {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Commitment strategy used.
    pub tag: CommitmentStrategy,
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

// ── HybridVC ───────────────────────────────────────────────────────────────

/// Hybrid state commitment engine.
///
/// Dispatches between SSMC (for small columns) and SMT (for large columns)
/// based on a configurable threshold.
#[derive(Clone)]
pub struct HybridVC<H: FieldHasher> {
    hasher: H,
    threshold: usize,
}

impl<H: FieldHasher<F = BabyBear, Digest = NativeDigest>> HybridVC<H> {
    /// Create a new hybrid VC.
    ///
    /// Columns with ≤ `threshold` entries use SSMC; larger use SMT.
    pub fn new(hasher: H, threshold: usize) -> Self {
        Self { hasher, threshold }
    }

    /// The dispatch threshold.
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Choose commitment strategy based on entry count.
    pub fn strategy_for(&self, entry_count: usize) -> CommitmentStrategy {
        if entry_count <= self.threshold {
            CommitmentStrategy::Ssmc
        } else {
            CommitmentStrategy::Smt
        }
    }

    // ── Per-column operations ──────────────────────────────────────────────

    /// Commit a column from pre-encoded entries (must be sorted by key).
    ///
    /// Returns the column state and its commitment digest.
    pub fn commit_column(
        &self,
        table: TableId,
        col: ColId,
        entries: Vec<(RowKey, Vec<BabyBear>)>,
    ) -> (ColumnState<H>, NativeDigest) {
        if entries.len() <= self.threshold {
            let ssmc_entries: Vec<SsmcEntry> = entries
                .into_iter()
                .map(|(key, value)| SsmcEntry { key, value })
                .collect();
            let list = SsmcList::from_sorted(table, col, ssmc_entries)
                .expect("entries must be sorted by key");
            let com = list.commit(&self.hasher).0;
            (ColumnState::Ssmc(list), com)
        } else {
            let mut tree =
                SparseMerkleTree::new(self.hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);
            for (key, value_fes) in entries {
                let leaf = self.hasher.hash(&value_fes);
                tree.insert(key.0, leaf);
            }
            let root = tree.root();
            (ColumnState::Smt(tree), root)
        }
    }

    /// Get the current commitment of a column state.
    pub fn column_commitment(&self, state: &ColumnState<H>) -> NativeDigest {
        match state {
            ColumnState::Ssmc(list) => list.commit(&self.hasher).0,
            ColumnState::Smt(tree) => tree.root(),
        }
    }

    /// Apply writes to a column. Returns `(new_state, new_commitment, merge_trace)`.
    ///
    /// `writes` must be sorted by key. `None` value = delete.
    /// Merge trace is produced for SSMC columns; SMT columns return `None`.
    pub fn apply_column_writes(
        &self,
        old_state: &ColumnState<H>,
        table: TableId,
        col: ColId,
        writes: &[(RowKey, Option<Vec<BabyBear>>)],
    ) -> (ColumnState<H>, NativeDigest, Option<MergeTrace>) {
        match old_state {
            ColumnState::Ssmc(old_list) => {
                let (new_list, com, trace) =
                    SsmcList::merge(old_list, writes, table, col, &self.hasher);
                (ColumnState::Ssmc(new_list), com.0, Some(trace))
            }
            ColumnState::Smt(old_tree) => {
                let mut tree = old_tree.clone();
                for (key, value) in writes {
                    match value {
                        Some(fes) => {
                            let leaf = self.hasher.hash(fes);
                            tree.insert(key.0, leaf);
                        }
                        None => {
                            tree.remove(key.0);
                        }
                    }
                }
                let root = tree.root();
                (ColumnState::Smt(tree), root, None)
            }
        }
    }

    /// Commitment for an empty column: `Poseidon(DOMAIN_SSMC || t || c || 0)`.
    pub fn empty_commitment(&self, table: TableId, col: ColId) -> NativeDigest {
        SsmcList::new(table, col).commit(&self.hasher).0
    }

    // ── State root (two-level SMT) ────────────────────────────────────────

    /// Compute the ColumnMeta leaf digest.
    ///
    /// `Poseidon(DOMAIN_LEAF || t || c || tag || Com[0..8])`
    pub fn compute_leaf(
        &self,
        table: TableId,
        col: ColId,
        tag: CommitmentStrategy,
        commitment: &NativeDigest,
    ) -> NativeDigest {
        let tag_val: u32 = match tag {
            CommitmentStrategy::Ssmc => 0,
            CommitmentStrategy::Smt => 1,
        };
        let mut input = Vec::with_capacity(3 + 8);
        input.push(BabyBear::new(table.0));
        input.push(BabyBear::new(col.0 as u32));
        input.push(BabyBear::new(tag_val));
        input.extend_from_slice(&commitment.0);
        self.hasher.hash_domain(DOMAIN_LEAF, &input)
    }

    /// Build column-level SMT from column leaves.
    ///
    /// `SMT_cols(depth=16, domain=DOMAIN_COL)`.
    pub fn compute_table_root(&self, col_leaves: &BTreeMap<ColId, NativeDigest>) -> NativeDigest {
        let mut tree = SparseMerkleTree::new(self.hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
        for (&col, &leaf) in col_leaves {
            tree.insert(col.0 as u64, leaf);
        }
        tree.root()
    }

    /// Build table-level SMT from table roots.
    ///
    /// `SMT_tables(depth=32, domain=DOMAIN_TABLE)`.
    pub fn compute_state_root(
        &self,
        table_roots: &BTreeMap<TableId, NativeDigest>,
    ) -> NativeDigest {
        let mut tree =
            SparseMerkleTree::new(self.hasher.clone(), TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE);
        for (&table, &root) in table_roots {
            tree.insert(table.0 as u64, root);
        }
        tree.root()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hasher::MockFieldHasher;
    use p3_baby_bear::BabyBear;

    fn vc(threshold: usize) -> HybridVC<MockFieldHasher> {
        HybridVC::new(MockFieldHasher, threshold)
    }

    fn val(n: u32) -> Vec<BabyBear> {
        vec![BabyBear::new(n)]
    }

    fn entries(pairs: &[(u64, u32)]) -> Vec<(RowKey, Vec<BabyBear>)> {
        pairs.iter().map(|&(k, v)| (RowKey(k), val(v))).collect()
    }

    // ── Strategy dispatch ──────────────────────────────────────────────────

    #[test]
    fn strategy_dispatch_small_uses_ssmc() {
        let h = vc(5);
        let (state, _) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2)]));
        assert_eq!(state.strategy(), CommitmentStrategy::Ssmc);
    }

    #[test]
    fn strategy_dispatch_at_threshold_uses_ssmc() {
        let h = vc(3);
        let (state, _) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2), (2, 3)]));
        assert_eq!(state.strategy(), CommitmentStrategy::Ssmc);
    }

    #[test]
    fn strategy_dispatch_above_threshold_uses_smt() {
        let h = vc(2);
        let (state, _) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2), (2, 3)]));
        assert_eq!(state.strategy(), CommitmentStrategy::Smt);
    }

    // ── Leaf digest ────────────────────────────────────────────────────────

    #[test]
    fn leaf_digest_deterministic() {
        let h = vc(10);
        let (_, com) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 42)]));
        let l1 = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com);
        let l2 = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com);
        assert_eq!(l1, l2);
    }

    #[test]
    fn leaf_digest_changes_with_strategy_tag() {
        let h = vc(10);
        let com = NativeDigest::default();
        let l_ssmc = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com);
        let l_smt = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Smt, &com);
        assert_ne!(l_ssmc, l_smt);
    }

    // ── Table root ─────────────────────────────────────────────────────────

    #[test]
    fn table_root_single_column() {
        let h = vc(10);
        let (_, com) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1)]));
        let leaf = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com);
        let mut cols = BTreeMap::new();
        cols.insert(ColId(0), leaf);
        let root = h.compute_table_root(&cols);
        // Non-trivial: root differs from empty
        let empty_root = h.compute_table_root(&BTreeMap::new());
        assert_ne!(root, empty_root);
    }

    #[test]
    fn table_root_multiple_columns() {
        let h = vc(10);
        let (_, com0) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1)]));
        let (_, com1) = h.commit_column(TableId(1), ColId(1), entries(&[(0, 2)]));
        let leaf0 = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com0);
        let leaf1 = h.compute_leaf(TableId(1), ColId(1), CommitmentStrategy::Ssmc, &com1);

        let mut cols_both = BTreeMap::new();
        cols_both.insert(ColId(0), leaf0);
        cols_both.insert(ColId(1), leaf1);

        let mut cols_one = BTreeMap::new();
        cols_one.insert(ColId(0), leaf0);

        assert_ne!(
            h.compute_table_root(&cols_both),
            h.compute_table_root(&cols_one)
        );
    }

    // ── State root ─────────────────────────────────────────────────────────

    #[test]
    fn state_root_single_table() {
        let h = vc(10);
        let root = NativeDigest::default();
        let mut tables = BTreeMap::new();
        tables.insert(TableId(1), root);
        let state_root = h.compute_state_root(&tables);
        assert_ne!(state_root, h.compute_state_root(&BTreeMap::new()));
    }

    #[test]
    fn state_root_multiple_tables() {
        let h = vc(10);
        let mut tables = BTreeMap::new();
        tables.insert(TableId(1), NativeDigest([BabyBear::new(1); 8]));
        tables.insert(TableId(2), NativeDigest([BabyBear::new(2); 8]));

        let mut single = BTreeMap::new();
        single.insert(TableId(1), NativeDigest([BabyBear::new(1); 8]));

        assert_ne!(h.compute_state_root(&tables), h.compute_state_root(&single));
    }

    // ── apply_column_writes ────────────────────────────────────────────────

    #[test]
    fn apply_writes_ssmc_updates_commitment() {
        let h = vc(10);
        let (state, com_old) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1)]));
        let writes = vec![(RowKey(1), Some(val(2)))];
        let (new_state, com_new, trace) =
            h.apply_column_writes(&state, TableId(1), ColId(0), &writes);
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.strategy(), CommitmentStrategy::Ssmc);
        assert!(trace.is_some());
        assert_eq!(trace.unwrap().steps.len(), 2); // old key 0 + new key 1
    }

    #[test]
    fn apply_writes_smt_updates_commitment() {
        let h = vc(1); // threshold=1 → 3 entries triggers SMT
        let (state, com_old) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2)]));
        assert_eq!(state.strategy(), CommitmentStrategy::Smt);

        let writes = vec![(RowKey(2), Some(val(3)))];
        let (new_state, com_new, trace) =
            h.apply_column_writes(&state, TableId(1), ColId(0), &writes);
        assert_ne!(com_old, com_new);
        assert_eq!(new_state.strategy(), CommitmentStrategy::Smt);
        assert!(trace.is_none()); // SMT produces no merge trace
    }

    #[test]
    fn apply_writes_delete_removes_entry() {
        let h = vc(10);
        let (state, _) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2)]));
        let writes = vec![(RowKey(0), None)]; // delete key 0
        let (new_state, _, trace) = h.apply_column_writes(&state, TableId(1), ColId(0), &writes);
        assert!(!new_state.is_empty());
        let trace = trace.unwrap();
        // key 0 (Both/delete) + key 1 (OldOnly)
        assert_eq!(trace.steps.len(), 2);
        assert!(!trace.steps[0].in_new); // key 0 deleted
    }

    // ── ColumnMeta ─────────────────────────────────────────────────────────

    #[test]
    fn column_meta_fields_correct() {
        let h = vc(10);
        let (state, com_old) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1)]));
        let writes = vec![(RowKey(1), Some(val(2)))];
        let (new_state, com_new, _) = h.apply_column_writes(&state, TableId(1), ColId(0), &writes);

        let meta = ColumnMeta {
            table: TableId(1),
            col: ColId(0),
            tag: state.strategy(),
            com_old,
            com_new,
            is_empty_old: state.is_empty(),
            is_empty_new: new_state.is_empty(),
            is_touched: true,
        };

        assert_eq!(meta.tag, CommitmentStrategy::Ssmc);
        assert_ne!(meta.com_old, meta.com_new);
        assert!(!meta.is_empty_old);
        assert!(!meta.is_empty_new);
        assert!(meta.is_touched);
    }

    // ── Empty column ───────────────────────────────────────────────────────

    #[test]
    fn empty_commitment_matches_empty_ssmc() {
        let h = vc(10);
        let (state, com) = h.commit_column(TableId(1), ColId(0), entries(&[]));
        assert!(state.is_empty());
        assert_eq!(com, h.empty_commitment(TableId(1), ColId(0)));
    }

    #[test]
    fn empty_commitment_different_columns() {
        let h = vc(10);
        assert_ne!(
            h.empty_commitment(TableId(1), ColId(0)),
            h.empty_commitment(TableId(1), ColId(1)),
        );
    }

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    fn round_trip_ssmc_commit_then_apply_empty_writes() {
        let h = vc(10);
        let (state, com) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2)]));
        let (_, com_after, _) = h.apply_column_writes(&state, TableId(1), ColId(0), &[]);
        assert_eq!(com, com_after);
    }

    #[test]
    fn round_trip_smt_commit_then_apply_empty_writes() {
        let h = vc(1); // threshold=1 → 2 entries triggers SMT
        let (state, com) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 1), (1, 2)]));
        assert_eq!(state.strategy(), CommitmentStrategy::Smt);
        let (_, com_after, _) = h.apply_column_writes(&state, TableId(1), ColId(0), &[]);
        assert_eq!(com, com_after);
    }

    // ── Full state root pipeline ───────────────────────────────────────────

    #[test]
    fn full_state_root_pipeline() {
        let h = vc(10);

        // Commit two columns in table 1.
        let (_, com_t1c0) = h.commit_column(TableId(1), ColId(0), entries(&[(0, 10)]));
        let (_, com_t1c1) = h.commit_column(TableId(1), ColId(1), entries(&[(0, 20)]));

        // Build ColumnMeta leaves.
        let leaf_t1c0 = h.compute_leaf(TableId(1), ColId(0), CommitmentStrategy::Ssmc, &com_t1c0);
        let leaf_t1c1 = h.compute_leaf(TableId(1), ColId(1), CommitmentStrategy::Ssmc, &com_t1c1);

        // Table root.
        let mut cols = BTreeMap::new();
        cols.insert(ColId(0), leaf_t1c0);
        cols.insert(ColId(1), leaf_t1c1);
        let table_root = h.compute_table_root(&cols);

        // State root.
        let mut tables = BTreeMap::new();
        tables.insert(TableId(1), table_root);
        let state_root = h.compute_state_root(&tables);

        // Deterministic: recompute should match.
        let state_root_2 = h.compute_state_root(&tables);
        assert_eq!(state_root, state_root_2);

        // Non-trivial: different from empty.
        assert_ne!(state_root, h.compute_state_root(&BTreeMap::new()));
    }
}
