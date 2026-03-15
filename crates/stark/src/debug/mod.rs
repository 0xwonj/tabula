//! Debug constraint checker: verify AIR constraints and LogUp balance.
//!
//! Two levels of checking:
//!
//! 1. **Single-chip** ([`debug_check`], [`debug_check_all`]):
//!    Evaluates local + transition constraints on a concrete trace.
//!    Any nonzero constraint value is a violation.
//!
//! 2. **Multi-chip LogUp** ([`debug_check_logup`]):
//!    Evaluates all chips' constraints AND verifies that LogUp interactions
//!    balance across the entire system. Uses random challenges over the
//!    quartic extension field `KoalaBear⁴` for ~124-bit collision resistance.

mod builder;
mod errors;
mod logup;
mod single_chip;

pub use builder::DebugConstraintBuilder;
pub use errors::{ConstraintError, MultiChipError};
pub use logup::{
    ChipRecord, ChipTrace, RecordedInteraction, check_bus_balance, check_logup_balance,
    check_logup_balance_with_challenges, compute_fingerprint, debug_check_logup, evaluate_chip,
    evaluate_chip_interactions_only, evaluate_chip_with_preprocessed,
    evaluate_chip_with_preprocessed_and_public_values, evaluate_chip_with_public_values,
};
pub use single_chip::{
    debug_check, debug_check_all, debug_check_with_preprocessed,
    debug_check_with_preprocessed_and_public_values, debug_check_with_public_values,
};

#[cfg(test)]
mod tests;
