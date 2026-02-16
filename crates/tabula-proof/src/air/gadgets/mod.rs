//! Reusable AIR constraint gadgets.
//!
//! Each submodule provides pure functions generic over `AB: AirBuilder`
//! and optionally embeddable `#[repr(C)]` column structs (SP1 Operations pattern).
//!
//! Gadgets bundle three concerns:
//! - **Column struct**: embeddable in any chip's `Cols<T>` (when applicable)
//! - **`populate()`**: fill witness values from concrete data
//! - **`eval()`**: emit AIR constraints

pub mod boolean;
pub mod integer;
pub mod mem;

pub use boolean::constrain_is_real_prefix;
pub use integer::{
    IsZero, StrictIneq, U64Limbs, constrain_is_zero, constrain_strict_ineq,
    constrain_u64_decomposition,
};
pub use mem::{constrain_mem_read, constrain_mem_write, constrain_null_canon};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

/// Convert a `bool` to a `BabyBear` field element (1 or 0).
pub(crate) fn bool_fe(b: bool) -> BabyBear {
    if b { BabyBear::ONE } else { BabyBear::ZERO }
}
