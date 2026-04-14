//! ExecutionChip — instruction trace AIR.
//!
//! Module layout:
//! - `columns.rs`: `ExecutionCols<T, W>` column struct + width constant
//! - `air.rs`: `ExecutionChip` struct + `BaseAir` + `Air` (structural constraints + bus sends)
//! - `trace.rs`: `generate_execution_trace()` (witness -> trace matrix)
//! - `trace_utils.rs`: pure u64 <-> KoalaBear limb conversion utilities
//! - `populate.rs`: per-opcode witness population helpers
//! - `ops/`: per-opcode constraint modules (arith, mul, cmp, divmod, logic, control, hash)
//! - `linkage.rs`: operand-to-slot linkage constraints

pub mod air;
mod buses;
pub mod columns;
mod linkage;
pub(crate) mod ops;
mod populate;
mod range_checks;
pub mod trace;
pub mod trace_utils;

pub use air::ExecutionChip;
pub use columns::{
    EXECUTION_STANDARD_VALUE_WIDTH, EXECUTION_STANDARD_WIDTH, ExecutionCols, MAX_SLOTS,
    execution_width,
};
pub use trace::{CmpOp, InstructionRecord, Opcode, generate_execution_trace};
pub use trace_utils::{
    limbs_to_u64, native_key_payload_prefix3, native_key_payload_to_u64, u64_add_limbs,
    u64_sub_limbs, u64_to_limbs, u64_to_native_key_payload,
};
