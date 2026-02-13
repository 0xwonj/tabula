#![warn(missing_docs)]
#![deny(unused)]

//! Core types, traits, and error definitions for the Tabula kernel.

pub mod error;
pub mod event;
pub mod ir;
#[cfg(any(feature = "mock", test))]
pub mod mock;
pub mod schema;
pub mod state;
pub mod traits;
pub mod tx;
pub mod types;
