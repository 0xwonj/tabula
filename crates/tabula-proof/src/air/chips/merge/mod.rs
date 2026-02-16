//! GlobalMerge chip — 3-way merge proof table.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `GlobalMergeCols<T, W>` column struct + width constant
//! - `air.rs`: `GlobalMergeChip<W>` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_merge_trace()` (witness -> trace matrix) + tests

pub mod air;
pub mod columns;
pub mod trace;

pub use air::GlobalMergeChip;
pub use columns::{GlobalMergeCols, MERGE_STANDARD_WIDTH, merge_width};
pub use trace::{MergeRow, MergeSource, generate_merge_trace};
