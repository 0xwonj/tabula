//! ColumnMeta chip — tracks per-column commitment transitions.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `ColumnMetaCols<T>` column struct + width constant
//! - `air.rs`: `ColumnMetaChip` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_column_meta_trace()` (witness → trace matrix) + tests

pub mod air;
pub mod columns;
pub mod trace;

pub use air::ColumnMetaChip;
pub use columns::{COLUMN_META_WIDTH, ColumnMetaCols};
pub use trace::generate_column_meta_trace;
