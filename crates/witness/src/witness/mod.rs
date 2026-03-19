//! Witness pipeline helpers for shared execution-row preparation.
//!
//! - [`types`]: Shared row structures (`InitRow`, `AccessRow`)
//! - [`encoding`]: Value encoding and SSMC hash-chain inputs
//! - [`program_info`]: `ProgramInfo` — per-program metadata for proof optimization

pub(crate) mod encoding;
pub mod program_info;
pub(crate) mod types;

pub use types::{AccessRow, InitRow};
