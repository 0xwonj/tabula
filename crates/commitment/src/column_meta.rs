//! Column metadata types for the hybrid state commitment system.
//!
//! Contains the data types that describe per-column commitment state,
//! strategy dispatch, and batch transition metadata.

use tabula_core::{ColId, TableId};

use crate::field::NativeDigest;
use crate::hasher::FieldHasher;
use crate::smt::SparseMerkleTree;
use crate::ssmc::SsmcList;

// ── Scheme Tags ──────────────────────────────────────────────────────────────

/// Well-known scheme tag constants for commitment strategies.
///
/// Core schemes use values 0–9. Application-defined schemes should
/// use values >= 100 to avoid collisions.
pub mod scheme_tags {
    /// Small Sparse Map Commitment (hash chain).
    pub const SSMC: u16 = 0;
    /// Sparse Merkle Tree.
    pub const SMT: u16 = 1;
}

// ── Types ──────────────────────────────────────────────────────────────────

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

    /// Scheme tag for this column's commitment strategy.
    pub fn scheme_tag(&self) -> u16 {
        match self {
            ColumnState::Ssmc(_) => scheme_tags::SSMC,
            ColumnState::Smt(_) => scheme_tags::SMT,
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
    /// Scheme tag (see [`scheme_tags`]).
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
