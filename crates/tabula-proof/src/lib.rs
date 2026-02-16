#![warn(missing_docs)]
#![deny(unused)]

//! Proof generation and verification for the Tabula kernel.

pub mod statement;

#[cfg(feature = "stark")]
pub mod air;
#[cfg(feature = "stark")]
pub mod witness;

#[cfg(feature = "stark")]
pub use witness::{
    AccessPattern, AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute, LiteralCell,
    ProgramInfo, TemplateId, WitnessGenerator, route_keys,
};
