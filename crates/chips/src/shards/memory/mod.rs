//! Per-column memory shard chip.
//!
//! Replaces the global `InterTxOrderChip` for a single `(table_id, col_id)`.
//! By operating on one column at a time, eliminates:
//! - `SameKeyDetection` (5 columns) — no segment boundaries
//! - `LexOrderingDirection` (3 columns) — no cross-segment ordering
//!
//! Column budget: 48 (W=3) vs 56 for the global chip.

pub mod air;
pub mod columns;
pub mod trace;

pub use air::MemoryShardChip;
pub use columns::{MemoryShardCols, memory_shard_width};
pub use trace::{MemoryShardRow, generate_memory_shard_trace};
