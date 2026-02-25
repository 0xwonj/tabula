#![warn(missing_docs)]
#![deny(unused)]

//! Proof generation and verification for the Tabula kernel.

// Curated re-exports from tabula-contract (proof's public API surface).
pub use tabula_contract::{
    ContractCompatibilityPolicy, ContractMetadataEnvelope, PublicInputField, PublicInputs,
};

#[cfg(feature = "stark")]
pub mod air;
#[cfg(feature = "stark")]
pub mod chips;
#[cfg(feature = "stark")]
pub mod debug;
#[cfg(feature = "stark")]
pub mod gadgets;
#[cfg(feature = "stark")]
pub mod stark;
#[cfg(feature = "stark")]
pub mod trace;
#[cfg(feature = "stark")]
pub mod witness;

#[cfg(feature = "stark")]
pub use stark::StarkAir;

#[cfg(feature = "stark")]
pub use witness::{
    AccessPattern, AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute, LiteralCell,
    ProgramInfo, TemplateId, WitnessGenerator, route_keys,
};
