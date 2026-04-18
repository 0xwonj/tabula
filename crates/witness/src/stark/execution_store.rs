//! Execution-tier witness-store assembly for the current STARK backend.

use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
use tabula_core::error::TabulaError;

use tabula_chips::public_context_transcript::PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::tx_batch_transcript::TX_BATCH_TRANSCRIPT_WITNESS_LABEL;
use tabula_stark::trace::{WitnessStore, witness_labels};
use tabula_stark::witness_kit::KitFinalizeContext;

use super::kit_registry::ChipKitRegistry;
use super::lowering::LoweringOutput;

/// Build the execution-tier witness store from lowered execution inputs.
///
/// The `registry` drives each [`ChipWitnessKit`](tabula_stark::witness_kit::ChipWitnessKit)
/// over the shared scratchpad owned by `lowering`; kits publish their
/// rows under their canonical witness-store labels. Labels not owned
/// by any kit (core instruction records, static table rows, transcript
/// families) are published directly here and will migrate to kits in
/// subsequent SP-3 stages.
///
/// Callers that depend on runtime-pre-stuff kits (e.g. the relation
/// table kit, which reads rows derived from a prepared relation proof)
/// are responsible for inserting those rows into
/// `lowering.kit_scratch` before calling this function.
pub fn prepare_execution_store(
    lowering: &mut LoweringOutput,
    registry: &ChipKitRegistry,
) -> Result<WitnessStore, TabulaError> {
    let mut store = WitnessStore::new();
    store.put(
        witness_labels::EXECUTION_RECORDS,
        lowering.instruction_records.clone(),
    );
    store.put(
        witness_labels::STATIC_TABLE_ROWS,
        lowering.static_table_rows.clone(),
    );
    store.put(
        PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL,
        lowering.public_context_transcript_items.clone(),
    );
    store.put(
        TX_BATCH_TRANSCRIPT_WITNESS_LABEL,
        lowering.tx_batch_transcript_items.clone(),
    );
    store.put(
        EVENT_TRANSCRIPT_WITNESS_LABEL,
        lowering.event_transcript_items.clone(),
    );

    let mut ctx = KitFinalizeContext::new(&mut lowering.kit_scratch);
    for kit in registry.iter() {
        kit.finalize(&mut ctx, &mut store)
            .map_err(|err| TabulaError::ProofError {
                phase: "execution_store_assembly",
                detail: format!("chip witness kit finalize failed: {err}"),
            })?;
    }
    Ok(store)
}
