//! Generic trace validation via [`ChipSet`] dispatch.
//!
//! Replaces hardcoded per-chip `debug_check` + `evaluate_chip` calls with
//! a single generic loop over `CS::all_chips()`.

use p3_air::Air;
use p3_baby_bear::BabyBear;

use tabula_core::error::TabulaError;

use tabula_stark::air::chip_set::ChipSet;
use tabula_stark::debug::{
    DebugConstraintBuilder, check_bus_balance, evaluate_chip_with_preprocessed_and_public_values,
};

use super::TraceMap;

/// Validate all chip traces in a [`TraceMap`] with debug constraints and bus balance checks.
///
/// For each chip in `CS::all_chips()`:
/// 1. Evaluates AIR constraints (fails on first violated constraint).
/// 2. Records LogUp interaction sends/receives.
///
/// Then checks bus balance across all chips for every bus in `CS::bus_manifest()`.
pub(super) fn debug_validate_trace_map<CS>(map: &TraceMap) -> Result<(), TabulaError>
where
    CS: ChipSet + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>,
{
    let all_chips = CS::all_chips();
    let mut records = Vec::with_capacity(all_chips.len());

    for chip in &all_chips {
        let chip_id = chip.chip_id();
        let entry = map.get(chip_id).ok_or_else(|| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("{} trace must exist", chip_id),
        })?;

        let record = evaluate_chip_with_preprocessed_and_public_values(
            &chip_id.to_string(),
            chip,
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
    for bus in CS::bus_manifest() {
        check_bus_balance(&records, bus).map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("bus {} imbalance: {e}", bus),
        })?;
    }

    Ok(())
}
