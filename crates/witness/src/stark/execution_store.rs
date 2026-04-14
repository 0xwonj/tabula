//! Execution-tier witness-store assembly for the current STARK backend.

use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
use tabula_core::error::TabulaError;

use tabula_chips::ir_hash::IR_HASH_WITNESS_LABEL;
use tabula_chips::public_context_transcript::PUBLIC_CONTEXT_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::relation_table::RELATION_TABLE_WITNESS_LABEL;
use tabula_chips::relation_table::RelationTableWitnessRow;
use tabula_chips::relation_transcript::RELATION_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::tx_batch_transcript::TX_BATCH_TRANSCRIPT_WITNESS_LABEL;
use tabula_stark::trace::{WitnessStore, witness_labels};

use crate::PreparedRelationProof;

use super::lowering::LoweringOutput;

/// Build the execution-tier witness store from lowered execution inputs.
pub fn prepare_execution_store(
    lowering: &LoweringOutput,
    relation_proof: &PreparedRelationProof,
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
    store.put(IR_HASH_WITNESS_LABEL, lowering.ir_hash_calls.clone());
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
    store.put(
        RELATION_TRANSCRIPT_WITNESS_LABEL,
        lowering.relation_transcript_calls.clone(),
    );
    store.put(
        RELATION_TABLE_WITNESS_LABEL,
        relation_proof
            .table_rows()
            .iter()
            .map(|row| RelationTableWitnessRow {
                relation_id: row.relation_id,
                input_digest: row.input_digest,
                output_digest: row.output_digest,
                lookup_mult: row.lookup_mult,
            })
            .collect::<Vec<_>>(),
    );
    Ok(store)
}
