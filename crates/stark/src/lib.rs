//! STARK foundation crate for the Tabula proof system.
//!
//! Defines the core AIR constraint framework, chip identification types,
//! trace storage, debug constraint checker, and permutation trace generation.
//! No chip implementations — those live in downstream crates.

use p3_field::extension::BinomialExtensionField;
use p3_koala_bear::KoalaBear;

mod bridge;

#[macro_use]
pub mod air;
pub mod chips;
pub mod debug;
pub mod permutation;
pub mod rap;
pub mod trace;

/// Quartic extension of KoalaBear for ~124-bit security.
///
/// Used for LogUp fingerprints and permutation trace values.
pub type EF4 = BinomialExtensionField<KoalaBear, 4>;
