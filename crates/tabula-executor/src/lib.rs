#![warn(missing_docs)]
#![deny(unused)]

//! Deterministic execution engine for the Tabula kernel.

pub mod batch;
pub mod consistency;
pub mod interpreter;
pub mod overlay;
pub mod program;
pub mod resolve;

#[cfg(test)]
pub(crate) mod test_fixtures;
#[cfg(test)]
mod proptest_tests;
