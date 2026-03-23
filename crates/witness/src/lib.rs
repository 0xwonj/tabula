//! Witness preparation helpers for the Tabula proof system.
//!
//! The crate root exposes only the stable logical preparation seam used by
//! runtime-owned proof assembly. Current STARK-specific lowering and witness
//! assembly helpers live under [`stark`].

pub mod prepare;
pub mod stark;
mod types;

pub use prepare::{ExecutionInputPreparer, PreparedExecutionColumn, PreparedExecutionColumns};
pub use types::{
    AccessEvent, ColumnValueProfile, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim,
};
