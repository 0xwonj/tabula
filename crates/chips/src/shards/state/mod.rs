//! Per-column state shard chip.
//!
//! Replaces the global `StateColumnChip` for a single `(table_id, col_id)`.
//! Unifies SSMC (old commitment) and Merge (old + write → new) with two
//! parallel hash chains computing Com_old and Com_new.
//!
//! By operating on one column at a time, eliminates:
//! - `SameKeyDetection` (5 columns) — no segment boundaries
//! - `LexOrderingDirection` (3 columns) — no cross-segment ordering
//!
//! Column budget: 93 (W=3) vs 101 for the global chip.

pub mod air;
pub mod buses;
pub mod columns;
pub mod derived;
pub mod trace;

pub use air::StateShardChip;
pub use columns::{StateShardCols, state_shard_width};
pub use trace::{EntrySource, StateShardRow, generate_state_shard_trace};
