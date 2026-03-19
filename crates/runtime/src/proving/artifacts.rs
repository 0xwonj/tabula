use tabula_machine::PublicStatement;
use tabula_witness::trace::builtin::lowering::LoweringOutput;

use crate::columns::BatchProofInput;
use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::program::RuntimeProgram;

use super::prepare::{convert_batch, prepare_batch_proof_input, to_batch_result};

/// Proof-preparation artifacts shared by statement-building and proving.
pub(crate) struct WitnessArtifacts {
    pub(crate) proof_input: BatchProofInput,
    pub(crate) air_statement: PublicStatement,
    pub(crate) lowering: LoweringOutput,
}

/// Prepare the batch proof input and lowering artifacts derived from one executed batch.
pub(crate) fn prepare_witness_artifacts(
    runtime_program: &RuntimeProgram,
    state: &tabula_artifact::StateSnapshot,
    batch_file: &tabula_artifact::TransactionBatch,
    executed: &ExecutedBatch,
) -> Result<WitnessArtifacts, RuntimeError> {
    let batch = convert_batch(batch_file)?;
    let batch_result = to_batch_result(executed);
    let prepared = prepare_batch_proof_input(runtime_program, state, &batch, &batch_result)?;
    let air_statement = prepared.proof_input.public_statement();

    Ok(WitnessArtifacts {
        proof_input: prepared.proof_input,
        air_statement,
        lowering: prepared.lowering,
    })
}
