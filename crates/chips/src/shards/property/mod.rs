//! PropertyVerifier shard chip for structural query verification.
//!
//! Receives from the `PROPERTY_READ` external bus and verifies property
//! query results against committed column state. Lives in Tier 2 (column
//! proof) alongside Memory/State/Meta shard chips.

pub mod air;
pub mod columns;
pub mod trace;

pub use air::PropertyVerifierChip;
pub use trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
