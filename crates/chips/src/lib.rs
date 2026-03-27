//! AIR chip implementations.
//!
//! Each chip is a small struct implementing `BaseAir` + `Air` + [`ChipSpec`].
//! Use [`core_dyn_chips()`] to obtain all core chips as boxed trait objects.

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub mod capability_transcript;
pub mod execution;
pub mod ir_hash;
pub mod poseidon;
pub mod range_check;
mod registry;
pub mod relation_table;
pub mod relation_transcript;
pub mod shards;
pub mod smt_path;
pub mod static_table;

// Re-export core chip identification types from tabula-stark.
pub use registry::{core_bus_consumers, core_dyn_chips};
pub use tabula_stark::chips::{ChipId, ChipSpec, core_chips};
