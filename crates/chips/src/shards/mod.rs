//! Per-column shard chip implementations.
//!
//! Shard chips replace global memory/state/meta chips with per-column instances.
//! Each shard operates on a single `(table_id, col_id)` pair, eliminating the
//! need for segment detection and lex ordering gadgets.
//!
//! ## Dynamic ChipId allocation
//!
//! Each commitment scheme allocates unique [`ChipId`]s for its per-column shard
//! chips via [`ChipIdAllocator`]. Core chips use IDs 0–99; shard allocation
//! starts at 100.

pub mod memory;
pub mod meta;
pub mod smt;
pub mod ssmc;
pub mod state;

pub use tabula_stark::chips::ChipIdAllocator;
