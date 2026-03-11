//! STARK foundation crate for the Tabula proof system.
//!
//! Defines the core AIR constraint framework, chip identification types,
//! trace storage, debug constraint checker, and permutation trace generation.
//! No chip implementations — those live in downstream crates.

use p3_baby_bear::BabyBear;
use p3_field::extension::BinomialExtensionField;

mod bridge;

#[macro_use]
pub mod air;
pub mod chips;
pub mod debug;
pub mod gadgets;
pub mod permutation;
pub mod rap;
pub mod trace;

/// Quartic extension of BabyBear for ~124-bit security.
///
/// Used for LogUp fingerprints and permutation trace values.
pub type EF4 = BinomialExtensionField<BabyBear, 4>;
