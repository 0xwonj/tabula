#![warn(missing_docs)]
#![deny(unused)]

//! STARK foundation crate for the Tabula proof system.
//!
//! Defines the core AIR constraint framework, chip identification types,
//! trace storage, and debug constraint checker. No chip implementations —
//! those live in downstream crates.

mod bridge;

#[macro_use]
pub mod air;
pub mod chips;
pub mod debug;
pub mod gadgets;
pub mod trace;
