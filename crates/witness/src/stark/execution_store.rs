//! Execution-tier witness-store assembly for the current STARK backend.

use tabula_core::error::TabulaError;

use tabula_stark::trace::{WitnessStore, witness_labels};
use tabula_stark::witness_kit::KitFinalizeContext;

use super::kit_registry::ChipKitRegistry;
use super::lowering::LoweringOutput;

/// Build the execution-tier witness store from lowered execution inputs.
///
/// Core rows owned by `LoweringOutput` (instruction records, static
/// table rows) are published directly. Every chip-specific label goes
/// through the [`ChipWitnessKit`](tabula_stark::witness_kit::ChipWitnessKit)
/// protocol: `registry` drives each registered kit's `finalize`, which
/// drains its entry in `lowering.kit_scratch` and publishes rows under
/// its canonical witness-store label.
///
/// Runtime-pre-stuff kits (relation table, transcript families) expect
/// the caller to install their row buffers in
/// `lowering.kit_scratch` before calling this function; inline-push
/// kits (IR hash, relation transcript) populate themselves as opcode
/// handlers execute during lowering.
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
