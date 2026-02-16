//! PoseidonChip — Poseidon2 permutation AIR.
//!
//! Canonical 3-file chip layout + constants:
//! - `constants.rs`: round constants, linear layers, step-by-step permutation
//! - `columns.rs`: `PoseidonCols<T>` column struct + width constant
//! - `air.rs`: `PoseidonChip` struct + `BaseAir` + `Air` (constraints)
//! - `trace.rs`: `generate_poseidon_trace()` (witness -> trace matrix) + tests

pub mod air;
pub mod columns;
pub mod constants;
pub mod trace;

pub use air::PoseidonChip;
pub use columns::{POSEIDON_WIDTH, PoseidonCols, poseidon_width};
pub use trace::generate_poseidon_trace;
