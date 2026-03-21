//! Generic trace validation via [`DynChip`] dispatch.

use tabula_core::error::TabulaError;

use crate::air::interaction::BusId;
use crate::debug::{check_bus_balance, evaluate_chip_with_preprocessed_and_public_values};

use super::{DynChip, TraceMap};

/// Validate all chip traces in a [`TraceMap`] with debug constraint and bus-balance checks.
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
            detail: format!("{chip_id} trace must exist"),
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
            detail: format!("{chip_id} validation failed: {e}"),
        })?;

        records.push(record);
    }

    for &bus in buses {
        check_bus_balance(&records, bus).map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("bus {bus} imbalance: {e}"),
        })?;
    }

    Ok(())
}
