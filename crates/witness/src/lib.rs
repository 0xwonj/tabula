//! Witness preparation helpers for the Tabula proof system.
//!
//! The crate root exposes only stable logical carrier types used by
//! runtime-owned proof assembly. STARK-specific lowering and witness assembly
//! helpers live under [`stark`].

mod model;
mod relation_proof;
pub mod stark;

pub use model::{
    AccessEvent, ColumnValueProfile, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim,
    RelationClaim, RelationClaimKind,
};
pub use relation_proof::{PreparedRelationProof, PreparedRelationTableRow, prepare_relation_proof};
