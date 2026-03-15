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
//! - [`key_rc`]: U64Limbs + half-decomposition for range checking (7 cols)
//! - [`ordering_rc`]: StrictIneq + half-decomposition for range checking (7 cols)
//! - [`hash_chain`]: Poseidon hash chain input composition (16 cols)

pub mod boolean;
pub mod hash_chain;
pub mod integer;
pub mod key_rc;
pub mod mem;
pub mod ordering_rc;

// ── Primitive gadget re-exports ──

pub use boolean::{constrain_constant_identity, constrain_is_real_prefix};
pub use integer::{
    IsZero, Limb2Bits, LimbHalves, StrictIneq, U64Limbs, constrain_is_zero, constrain_limb_halves,
    constrain_limb2_bits, constrain_strict_ineq, constrain_u64_decomposition,
};
pub use mem::{constrain_mem_read, constrain_mem_write, constrain_null_canon};

// ── Composite operation re-exports ──

pub use hash_chain::{HashChainInput, constrain_hash_chain_input, constrain_hash_chain_transition};
pub use key_rc::{
    KeyRangeChecked, constrain_key_halves, eval_key_range_checked, send_key_range_checks,
};
pub use ordering_rc::{
    OrderingRangeChecked, constrain_ordering_halves, eval_ordering_range_checked,
    send_ordering_range_checks,
};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

/// Convert a `bool` to a `KoalaBear` field element (1 or 0).
pub fn bool_fe(b: bool) -> KoalaBear {
    if b { KoalaBear::ONE } else { KoalaBear::ZERO }
}
