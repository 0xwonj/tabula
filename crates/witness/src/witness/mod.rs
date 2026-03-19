//! Witness pipeline helpers for shared execution-row preparation.
//!
//! - [`types`]: Shared row structures (`InitRow`, `AccessRow`)
//! - [`encoding`]: Value encoding, SSMC hash-chain, column commitments, state root
//! - [`program_info`]: `ProgramInfo` — per-program metadata for proof optimization

pub(crate) mod encoding;
pub mod program_info;
pub(crate) mod types;

pub use encoding::proof_column_commitment;
pub use program_info::{LiteralCell, ProgramInfo, TemplateId};
pub use types::{AccessRow, InitRow};
