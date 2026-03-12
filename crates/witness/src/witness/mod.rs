//! Witness pipeline: transforms executor output into structured proof witness data.
//!
//! - [`types`]: Data structures (`InitRow`, `AccessRow`, `ColumnWitness`, `BatchWitness`)
//! - [`generator`]: `WitnessGenerator` — orchestrates BatchResult → BatchWitness
//! - [`encoding`]: Value encoding, SSMC hash-chain, column commitments, state root
//! - [`route`]: `KeyRoute` — classifies keys for memory-layer proof path selection
//! - [`program_info`]: `ProgramInfo` — per-program metadata for proof optimization

mod encoding;
mod generator;
pub mod program_info;
pub mod route;
pub mod types;

pub use generator::WitnessGenerator;
pub use program_info::{LiteralCell, ProgramInfo, TemplateId};
pub use route::{AccessPattern, KeyRoute, route_keys};
pub use types::{AccessRow, BatchWitness, ColumnWitness, InitRow};
