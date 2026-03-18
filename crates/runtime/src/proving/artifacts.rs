use tabula_commitment::PoseidonHasher;
use tabula_core::BatchResult;
use tabula_machine::PublicStatement;
use tabula_witness::BatchWitness;

use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::program::RuntimeProgram;

use super::witness::{
    build_old_column_states, extract_statement, generate_witness, to_batch_result,
};

/// Proof-preparation artifacts shared by statement-building and proving.
pub(crate) struct WitnessArtifacts {
    pub(crate) batch_result: BatchResult,
    pub(crate) witness: BatchWitness<PoseidonHasher>,
    pub(crate) air_statement: PublicStatement,
}

/// Prepare the witness-level artifacts derived from one executed batch.
pub(crate) fn prepare_witness_artifacts(
    runtime_program: &RuntimeProgram,
    state: &tabula_artifact::StateSnapshot,
    executed: &ExecutedBatch,
) -> Result<WitnessArtifacts, RuntimeError> {
    let old_column_states = build_old_column_states(runtime_program, state)?;
    let batch_result = to_batch_result(executed);
    let witness = generate_witness(
        &batch_result,
        runtime_program.schemas_by_id(),
        &old_column_states,
    )?;
    let air_statement = extract_statement(&witness);

    Ok(WitnessArtifacts {
        batch_result,
        witness,
        air_statement,
    })
}
