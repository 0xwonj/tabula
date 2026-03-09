//! Generic trace validation via [`DynChip`] dispatch.
//!
//! Replaces hardcoded per-chip `debug_check` + `evaluate_chip` calls with
//! a single loop over a chip slice.

use tabula_core::error::TabulaError;

use tabula_stark::air::interaction::BusId;
use tabula_stark::debug::{check_bus_balance, evaluate_chip_with_preprocessed_and_public_values};
use tabula_stark::trace::DynChip;

use super::TraceMap;

/// Validate all chip traces in a [`TraceMap`] with debug constraints and bus balance checks.
///
/// For each chip:
/// 1. Evaluates AIR constraints (fails on first violated constraint).
/// 2. Records LogUp interaction sends/receives.
///
/// Then checks bus balance across all chips for every bus in the provided manifest.
///
/// # Usage
///
/// External callers should prefer [`TabulaMachine::debug_validate()`] which
/// delegates to this function with the machine's own chip and bus configuration.
///
/// [`TabulaMachine::debug_validate()`]: tabula_machine::TabulaMachine::debug_validate
pub fn debug_validate_trace_map(
    chips: &[Box<dyn DynChip>],
    buses: &[BusId],
    map: &TraceMap,
) -> Result<(), TabulaError> {
    let mut records = Vec::with_capacity(chips.len());

    for chip in chips {
        let chip_id = chip.chip_id();
        let entry = map.get(chip_id).ok_or_else(|| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("{} trace must exist", chip_id),
        })?;

        let record = evaluate_chip_with_preprocessed_and_public_values(
            &chip_id.to_string(),
            chip.as_ref(),
            &entry.main,
            entry.preprocessed.as_ref(),
            &entry.public_values,
        )
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("{} validation failed: {e}", chip_id),
        })?;

        records.push(record);
    }

    // Bus balance checks across all chips.
    for &bus in buses {
        check_bus_balance(&records, bus).map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("bus {} imbalance: {e}", bus),
        })?;
    }

    Ok(())
}
