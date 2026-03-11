//! Generic trace orchestration via [`DynChip`] and [`BusConsumer`] dispatch.
//!
//! Replaces hardcoded per-chip trace building with phase-ordered dispatch.
//! Each chip pulls its own inputs from a [`WitnessStore`] and inserts its
//! trace into the [`TraceMap`].
//!
//! Bus-driven collection: instead of hardcoded Poseidon/RangeCheck collectors,
//! the orchestrator evaluates upstream traces and dispatches interactions to
//! [`BusConsumer`] implementations.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;

use tabula_stark::debug::evaluate_chip_with_preprocessed_and_public_values;
use tabula_stark::trace::{BusConsumer, DynChip, TracePhase, WitnessStore};

use super::TraceMap;

/// Build all chip traces into a [`TraceMap`] via [`DynChip`] dispatch.
///
/// Dispatches chips in phase order (Independent → Memory → Dependent).
/// Between Memory and Dependent phases, evaluates Phase 0+1 chip constraints
/// and dispatches interaction data to [`BusConsumer`] implementations.
///
/// # Usage
///
/// External callers should prefer [`TabulaMachine::build_traces()`] which
/// delegates to this function with the machine's own chip configuration.
/// Direct usage is for advanced scenarios (e.g., custom orchestration or
/// testing with a hand-picked chip subset).
///
/// [`TabulaMachine::build_traces()`]: tabula_machine::TabulaMachine::build_traces
pub fn build_all_traces(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    store: WitnessStore,
) -> Result<TraceMap, TabulaError> {
    build_traces_core(chips, bus_consumers, store)
}

/// Core trace building logic shared by `build_all_traces` and `build_traces_for`.
fn build_traces_core(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    mut store: WitnessStore,
) -> Result<TraceMap, TabulaError> {
    let mut map = TraceMap::new();

    // Group chips by phase for ordered dispatch.
    let mut by_phase: BTreeMap<TracePhase, Vec<&dyn DynChip>> = BTreeMap::new();
    for chip in chips {
        by_phase
            .entry(chip.phase())
            .or_default()
            .push(chip.as_ref());
    }

    for (phase, phase_chips) in &by_phase {
        // Before Dependent phase: collect interaction data from Phase 0+1 traces.
        if *phase == TracePhase::DEPENDENT {
            collect_via_bus_consumers(chips, bus_consumers, &map, &mut store)?;
        }

        for chip in phase_chips {
            chip.contribute(&store, &mut map)?;
        }
    }

    Ok(map)
}

/// Evaluate Phase 0+1 chip traces and dispatch interactions to [`BusConsumer`]s.
///
/// Replaces the previous hardcoded `collect_poseidon_inputs` / `collect_range_check_multiplicities`.
fn collect_via_bus_consumers(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    map: &TraceMap,
    store: &mut WitnessStore,
) -> Result<(), TabulaError> {
    // Collect all interactions from Phase 0+1 chip traces.
    let mut all_interactions = Vec::new();

    for chip in chips {
        if chip.phase() >= TracePhase::DEPENDENT {
            continue;
        }

        let chip_id = chip.chip_id();
        let entry = map.get(chip_id).ok_or_else(|| TabulaError::ProofError {
            phase: "trace_build",
            detail: format!("chip '{chip_id}' trace must exist for interaction collection"),
        })?;

        let record = evaluate_chip_with_preprocessed_and_public_values(
            &chip_id.to_string(),
            chip.as_ref(),
            &entry.main,
            entry.preprocessed.as_ref(),
            &entry.public_values,
        )
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_build",
            detail: format!("{chip_id} trace invalid: {e}"),
        })?;

        all_interactions.extend(record.interactions);
    }

    // Dispatch to each BusConsumer.
    for consumer in bus_consumers {
        consumer.collect(&all_interactions, store)?;
    }

    Ok(())
}
