#![warn(missing_docs)]
#![deny(unused)]

//! Proof generation and verification for the Tabula kernel.

pub mod mock;
pub mod opening;
pub mod statement;
pub mod traits;
pub mod update;

#[cfg(feature = "stark")]
mod trace;
#[cfg(feature = "stark")]
mod witness;

#[cfg(feature = "stark")]
pub use trace::{AccessRow, BatchWitness, ColumnWitness, InitRow};
#[cfg(feature = "stark")]
pub use witness::WitnessGenerator;
