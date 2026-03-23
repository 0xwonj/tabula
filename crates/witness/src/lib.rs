//! Witness preparation helpers for the Tabula proof system.
//!
//! The crate root exposes only stable logical carrier types used by
//! runtime-owned proof assembly. STARK-specific lowering and witness assembly
//! helpers live under [`stark`].

pub mod stark;
mod types;

pub use types::{
    AccessEvent, ColumnValueProfile, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim,
};
