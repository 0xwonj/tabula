//! Canonical high-level testing fixtures shared across workspace crates.
//!
//! Governance rules:
//! - fixtures stay black-box and scenario-oriented
//! - new fixtures need at least two generic consumers
//! - adapter-specific helpers do not belong here
//! - crate-local white-box seams stay in the owning crate

pub mod artifacts;
pub mod batch;
pub mod cases;
pub mod compiled;
pub mod examples;
pub mod programs;
pub mod schema;
pub mod state;
