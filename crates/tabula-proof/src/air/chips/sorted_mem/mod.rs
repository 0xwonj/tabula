//! GlobalSortedMem chip — sorted memory consistency table.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `GlobalSortedMemCols<T, W>` column struct + width constant
//! - `air.rs`: `GlobalSortedMemChip<W>` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_sorted_mem_trace()` (witness -> trace matrix) + tests

pub mod air;
pub mod columns;
pub mod trace;

pub use air::GlobalSortedMemChip;
pub use columns::{GlobalSortedMemCols, SORTED_MEM_STANDARD_WIDTH, sorted_mem_width};
pub use trace::{SortedMemRow, generate_sorted_mem_trace};
