//! GlobalSSMC chip — sorted-set membership commitment table.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `GlobalSsmcCols<T, W>` column struct + width constant
//! - `air.rs`: `GlobalSsmcChip<W>` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_ssmc_trace()` (witness -> trace matrix) + tests

pub mod air;
pub mod columns;
pub mod trace;

pub use air::GlobalSsmcChip;
pub use columns::{GlobalSsmcCols, SSMC_STANDARD_WIDTH, ssmc_width};
pub use trace::{SsmcEntry, generate_ssmc_trace};
