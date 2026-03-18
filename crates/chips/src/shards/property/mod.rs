//! SSMC property shard chip.
//!
//! This module holds the scheme-owned property verifier for SSMC-backed
//! columns. It consumes execution-side `PROPERTY_READ` claims together with
//! old-state anchors emitted by the SSMC column tiers.

pub mod air;
pub mod columns;
pub mod trace;

pub use air::SsmcPropertyChip;
pub use trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
