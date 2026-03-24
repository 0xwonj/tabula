//! Execution-tier witness-store assembly for the current STARK backend.

use tabula_core::error::TabulaError;

use tabula_chips::ir_hash::IR_HASH_WITNESS_LABEL;
use tabula_stark::trace::{WitnessStore, witness_labels};

use super::lowering::LoweringOutput;

/// Build the execution-tier witness store from lowered execution inputs.
pub fn prepare_execution_store(lowering: &LoweringOutput) -> Result<WitnessStore, TabulaError> {
    let mut store = WitnessStore::new();
    store.put(
        witness_labels::EXECUTION_RECORDS,
        lowering.instruction_records.clone(),
    );
    store.put(
        witness_labels::STATIC_TABLE_ROWS,
        lowering.static_table_rows.clone(),
    );
    store.put(IR_HASH_WITNESS_LABEL, lowering.ir_hash_calls.clone());
    Ok(store)
}
