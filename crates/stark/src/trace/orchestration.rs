//! Generic trace orchestration via [`DynChip`] and [`BusConsumer`] dispatch.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;

use crate::debug::evaluate_chip_with_preprocessed_and_public_values;

use super::{BusConsumer, DynChip, TraceMap, TracePhase, WitnessStore};

/// Build all chip traces into a [`TraceMap`] via [`DynChip`] dispatch.
pub fn build_all_traces(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    store: WitnessStore,
) -> Result<TraceMap, TabulaError> {
    build_traces_core(chips, bus_consumers, store)
}

fn build_traces_core(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    mut store: WitnessStore,
) -> Result<TraceMap, TabulaError> {
    let mut map = TraceMap::new();

    let mut by_phase: BTreeMap<TracePhase, Vec<&dyn DynChip>> = BTreeMap::new();
    for chip in chips {
        by_phase
            .entry(chip.phase())
            .or_default()
            .push(chip.as_ref());
    }

    for (phase, phase_chips) in &by_phase {
        if *phase == TracePhase::DEPENDENT {
            collect_via_bus_consumers(chips, bus_consumers, &map, &mut store)?;
        }

        for chip in phase_chips {
            chip.contribute(&store, &mut map)?;
        }
    }

    Ok(map)
}

fn collect_via_bus_consumers(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    map: &TraceMap,
    store: &mut WitnessStore,
) -> Result<(), TabulaError> {
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

    for consumer in bus_consumers {
        consumer.collect(&all_interactions, store)?;
    }

    Ok(())
}
