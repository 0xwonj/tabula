//! Column metadata types for the hybrid state commitment system.
//!
//! Contains the data types that describe per-column commitment state,
//! strategy dispatch, and batch transition metadata.

use tabula_core::{ColId, TableId};

use crate::field::NativeDigest;
use crate::hasher::FieldHasher;
use crate::smt::SparseMerkleTree;
use crate::ssmc::SsmcList;

// ── Types ──────────────────────────────────────────────────────────────────

/// Strategy used for a column's commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentStrategy {
    /// Small Sparse Map Commitment (hash chain). Used when entry count <= threshold.
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
