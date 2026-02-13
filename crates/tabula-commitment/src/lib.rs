#![warn(missing_docs)]
#![deny(unused)]

//! State commitment layer for the Tabula kernel.

pub mod column;
#[cfg(any(feature = "mock", test))]
pub mod mock;
pub mod opening_plan;
pub mod root;
pub mod table;
