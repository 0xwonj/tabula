//! Generic trace orchestration via [`TraceContributor`] dispatch.
//!
//! Replaces hardcoded per-chip trace building with phase-ordered dispatch.
//! Each chip pulls its own inputs from a [`WitnessStore`] and inserts its
//! trace into the [`TraceMap`].

use std::collections::BTreeMap;

use p3_air::Air;
use p3_baby_bear::BabyBear;

use tabula_core::error::TabulaError;

use tabula_stark::air::chip_set::ChipSet;
use tabula_stark::debug::{
    DebugConstraintBuilder, evaluate_chip_with_preprocessed_and_public_values,
};

use tabula_stark::trace::{TraceContributor, TracePhase, WitnessStore, witness_labels};

use super::TraceMap;
use super::collectors::{collect_poseidon_inputs, collect_range_check_multiplicities};

/// Build all chip traces into a [`TraceMap`] via [`TraceContributor`] dispatch.
///
/// Dispatches chips in phase order (Independent → Memory → Dependent).
/// Between Memory and Dependent phases, evaluates Phase 0+1 chip constraints
/// to collect Poseidon/RangeCheck inputs for Dependent-phase chips.
pub(super) fn build_all_traces<CS>(mut store: WitnessStore) -> Result<TraceMap, TabulaError>
where
    CS: ChipSet + TraceContributor + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>,
{
    let all_chips = CS::all_chips();
    let mut map = TraceMap::new();

    // Group chips by phase for ordered dispatch.
    let mut by_phase: BTreeMap<TracePhase, Vec<&CS>> = BTreeMap::new();
    for chip in &all_chips {
        by_phase.entry(chip.phase()).or_default().push(chip);
    }

    for (phase, chips) in &by_phase {
        // Before Dependent phase: collect interaction data from Phase 0+1 traces.
        if *phase == TracePhase::Dependent {
            collect_dependent_inputs::<CS>(&all_chips, &map, &mut store)?;
        }

        for chip in chips {
            chip.contribute(&store, &mut map)?;
        }
    }

    Ok(map)
}

/// Evaluate Phase 0+1 chip traces and collect Poseidon/RangeCheck sends.
///
/// This is the single point where AIR constraint evaluation occurs during
/// trace building. The collected data is inserted into the store for
/// Dependent-phase chips (Poseidon, RangeCheck).
fn collect_dependent_inputs<CS>(
    all_chips: &[CS],
    map: &TraceMap,
    store: &mut WitnessStore,
) -> Result<(), TabulaError>
where
    CS: ChipSet + TraceContributor + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>,
{
    let mut records = Vec::new();

    for chip in all_chips {
        if chip.phase() >= TracePhase::Dependent {
            continue;
        }

        let chip_id = chip.chip_id();
        let entry = map.get(chip_id).ok_or_else(|| TabulaError::ProofError {
            phase: "trace_build",
            detail: format!(
                "chip '{}' trace must exist for interaction collection",
                chip_id
            ),
        })?;

        let record = evaluate_chip_with_preprocessed_and_public_values(
            &chip_id.to_string(),
            chip,
            &entry.main,
            entry.preprocessed.as_ref(),
            &entry.public_values,
        )
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_build",
            detail: format!("{} trace invalid: {e}", chip_id),
        })?;

        records.push(record);
    }

    let record_refs: Vec<_> = records.iter().collect();
    let poseidon_inputs = collect_poseidon_inputs(&record_refs)?;
    let range_check_mults = collect_range_check_multiplicities(&record_refs)?;

    store.put(witness_labels::POSEIDON_INPUTS, poseidon_inputs);
    store.put(
        witness_labels::RANGE_CHECK_MULTS,
        Box::new(range_check_mults),
    );

    Ok(())
}
