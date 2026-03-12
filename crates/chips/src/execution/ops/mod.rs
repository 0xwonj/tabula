//! Per-opcode constraint modules for the ExecutionChip.
//!
//! Each module contains the AIR constraints for one opcode family.
//! All functions are `pub(crate)` — called from `air.rs`.

pub(crate) mod arith;
pub(crate) mod cmp;
pub(crate) mod control;
pub(crate) mod divmod;
pub(crate) mod hash;
pub(crate) mod logic;
pub(crate) mod mul;
pub(crate) mod precompile;
pub(crate) mod property_read;
