//! MetaShard chip — per-column commitment metadata.
//!
//! Per-column version of `MetaShardChip`. Each instance handles exactly one
//! `(table_id, col_id)` pair, eliminating lex ordering and IsZero gadgets.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `MetaShardCols<T>` column struct + width constant
//! - `air.rs`: `MetaShardChip` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_meta_shard_trace()` (witness → trace matrix)

pub mod air;
pub mod columns;
pub mod trace;

pub use air::MetaShardChip;
pub use columns::{META_SHARD_WIDTH, MetaShardCols};
pub use trace::{MetaShardRow, generate_meta_shard_trace};
