//! RangeCheckChip — preprocessed lookup table for range checks.
//!
//! Contains values `[0, 2^16)` with a multiplicity column for LogUp.
//! No AIR constraints needed — the table is preprocessed (fixed at setup time).
//! LogUp bus `core_buses::RANGE_CHECK` wired in M9.
//!
//! Other chips decompose values into sub-limbs and send range-check requests:
//! - u64 limbs (30 bits) → two 15-bit halves → two lookups each in [0, 2^16)
//! - u64 top limb (4 bits) → single lookup in [0, 16) ⊂ [0, 2^16)
//! - StrictIneq gap limbs → same decomposition

pub mod air;
pub mod columns;
pub mod trace;

pub use air::RangeCheckChip;
pub use columns::{RANGE_CHECK_SIZE, RANGE_CHECK_WIDTH, RangeCheckCols};
pub use trace::{generate_range_check_preprocessed, generate_range_check_trace};
