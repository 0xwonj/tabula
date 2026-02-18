//! ExecutionChip — instruction trace AIR.
//!
//! Canonical 3-file chip layout:
//! - `columns.rs`: `ExecutionCols<T, W>` column struct + width constant
//! - `air.rs`: `ExecutionChip` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_execution_trace()` (witness -> trace matrix) + tests

pub mod air;
pub mod columns;
pub mod trace;

pub use air::ExecutionChip;
pub use columns::{EXECUTION_STANDARD_WIDTH, ExecutionCols, MAX_SLOTS, execution_width};
pub use trace::{InstructionRecord, Opcode, generate_execution_trace, u64_to_limbs};
