#![warn(missing_docs)]
#![deny(unused)]

//! Proof generation and verification for the Tabula kernel.

pub use tabula_contract as contract;
pub mod statement;

#[cfg(feature = "stark")]
pub mod air;
#[cfg(feature = "stark")]
pub mod stark;
#[cfg(feature = "stark")]
pub mod trace_builder;
#[cfg(feature = "stark")]
pub mod witness;

#[cfg(feature = "stark")]
pub use witness::{
    AccessPattern, AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute, LiteralCell,
    ProgramInfo, TemplateId, WitnessGenerator, route_keys,
};
