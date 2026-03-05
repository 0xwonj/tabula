#![warn(missing_docs)]
#![deny(unused)]

//! Reusable AIR constraint gadgets.
//!
//! Each submodule provides pure functions generic over `AB: AirBuilder`
//! and optionally embeddable `#[repr(C)]` column structs (SP1 Operations pattern).
//!
//! Gadgets bundle three concerns:
//! - **Column struct**: embeddable in any chip's `Cols<T>` (when applicable)
//! - **`populate()`**: fill witness values from concrete data
//! - **`eval()`**: emit AIR constraints
//!
//! ## Primitive gadgets (single concern)
//!
//! - [`boolean`]: `is_real` prefix constraint
//! - [`integer`]: U64 limbs, half-decomposition, IsZero, StrictIneq
//! - [`mem`]: null canonicality, read/write transitions
//!
//! ## Composite operations (bundle columns + constraints + range checks)
//!
//! - [`segment`]: Same-(t,c) detection via IsZero (5 cols)
//! - [`lex`]: Lex ordering direction at segment boundaries (3 cols)
//! - [`key_rc`]: U64Limbs + half-decomposition for range checking (7 cols)
//! - [`ordering_rc`]: StrictIneq + half-decomposition for range checking (7 cols)
//! - [`hash_chain`]: Poseidon hash chain input composition (16 cols)

pub mod boolean;
pub mod hash_chain;
pub mod integer;
pub mod key_rc;
pub mod lex;
pub mod mem;
pub mod ordering_rc;
pub mod segment;

// ── Primitive gadget re-exports ──

pub use boolean::constrain_is_real_prefix;
pub use integer::{
    IsZero, Limb2Bits, LimbHalves, StrictIneq, U64Limbs, constrain_is_zero, constrain_limb_halves,
    constrain_limb2_bits, constrain_strict_ineq, constrain_u64_decomposition,
};
pub use mem::{constrain_mem_read, constrain_mem_write, constrain_null_canon};

// ── Composite operation re-exports ──

pub use hash_chain::{HashChainInput, constrain_hash_chain_input, constrain_hash_chain_transition};
pub use key_rc::{KeyRangeChecked, constrain_key_halves, send_key_range_checks};
pub use lex::{LexOrderingDirection, constrain_lex_direction, send_lex_range_checks};
pub use ordering_rc::{
    OrderingRangeChecked, constrain_ordering_halves, send_ordering_range_checks,
};
pub use segment::{SameKeyDetection, constrain_same_key_detection};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

/// Convert a `bool` to a `BabyBear` field element (1 or 0).
pub fn bool_fe(b: bool) -> BabyBear {
    if b { BabyBear::ONE } else { BabyBear::ZERO }
}
