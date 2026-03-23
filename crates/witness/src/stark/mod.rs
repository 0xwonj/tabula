//! STARK-specific witness lowering and witness-store assembly helpers.

pub mod lowering;
mod memory;
mod root_paths;
mod rows;
pub mod schemes;
pub mod shared_store;

pub(crate) use rows::{AccessRow, InitRow};

pub use lowering::{LowerProgramBatchInput, LoweringOutput, lower_program_batch};
pub use shared_store::{SharedStoreBuilder, SharedStoreContext};
