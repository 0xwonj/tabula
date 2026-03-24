mod artifacts;
mod batch_plan;
mod journal;
mod statement;

use serde::{Deserialize, Serialize};
use tabula_artifact::{State, Statement, TransactionBatch};
use tabula_ext::root::RootBackendBundle;
use tabula_machine::{PreparedMachineInput, TabulaProof};
use tabula_types::TypeRuntimeRegistry;

use crate::error::RuntimeError;
use crate::program::ResolvedProofProgram;

use artifacts::prepare_proof_artifacts;
pub(crate) use batch_plan::build_batch_proof_plan;
pub(crate) use journal::{JournalInput, build_proof_journal, convert_batch};
pub(crate) use statement::build_execution_statement;
pub use statement::digest_to_hex;

/// Final runtime-private proof preparation result before machine proving.
pub(crate) struct PreparedProofRequest {
    pub(crate) statement: Statement,
    pub(crate) machine_input: PreparedMachineInput,
}

pub(crate) fn prepare_proof_request(
    resolved_program: &ResolvedProofProgram,
    type_runtimes: &TypeRuntimeRegistry,
    root_backend_bundle: &RootBackendBundle,
    state: &State,
    batch_file: &TransactionBatch,
    state_after: &State,
    execution_journal: &tabula_executor::ExecutionJournal,
) -> Result<PreparedProofRequest, RuntimeError> {
    let batch = convert_batch(batch_file, type_runtimes)?;
    let static_tables = tabula_core::InMemoryStaticTables::new();
    let journal = build_proof_journal(JournalInput {
        resolved_program,
        state,
        batch: &batch,
        execution_journal,
        static_tables: &static_tables,
    })?;
    let batch_plan = build_batch_proof_plan(resolved_program, root_backend_bundle)?;
    let artifacts = prepare_proof_artifacts(resolved_program, &batch_plan, journal)?;
    let statement = build_execution_statement(
        resolved_program,
        state,
        batch_file,
        state_after,
        artifacts.air_statement(),
    )?;
    let machine_input = artifacts.into_prepared_machine_input(statement.statement_hash_bytes());

    Ok(PreparedProofRequest {
        statement,
        machine_input,
    })
}

/// Inputs for the proving pipeline.
///
/// Bundles the execution result with the original state and batch files
/// needed for witness generation and column state reconstruction.
pub struct ProveInput<'a> {
    /// Pre-execution state (for building `old_column_states`).
    pub state: &'a tabula_artifact::State,
    /// Transaction batch (for witness store preparation).
    pub batch: &'a tabula_artifact::TransactionBatch,
    /// Executed batch result (from [`run_batch`](crate::run_batch)).
    pub executed: &'a crate::execute::ExecutionEnvelope,
}

/// Result of STARK proof generation.
pub struct ProveResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Canonical execution statement bound into the proof transcript.
    pub statement: tabula_artifact::Statement,
    /// Summary of chips contributing to the proof.
    pub summary: ProofSummary,
}

/// Result of prove + verify.
pub struct VerifiedResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Canonical execution statement bound into the proof transcript.
    pub statement: tabula_artifact::Statement,
    /// Whether verification passed.
    pub verified: bool,
    /// Summary of chips contributing to the proof.
    pub summary: ProofSummary,
}

/// Summary of chip contributions in a STARK proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipSummary {
    /// Chip name.
    pub name: String,
    /// Trace height (number of rows).
    pub trace_height: usize,
}

/// Serializable summary of the chips contributing to one proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSummary {
    /// Per-chip summaries (name + trace height).
    pub chips: Vec<ChipSummary>,
    /// Total number of chips.
    pub chip_count: usize,
}

impl ProofSummary {
    /// Extract chip summaries from a [`TabulaProof`].
    pub fn from_proof(proof: &TabulaProof) -> Self {
        let chips = collect_chip_summaries(proof);
        Self {
            chip_count: chips.len(),
            chips,
        }
    }
}

fn collect_chip_summaries(proof: &TabulaProof) -> Vec<ChipSummary> {
    let mut chips = Vec::new();
    for opening in &proof.execution.chip_openings {
        chips.push(ChipSummary {
            name: opening.chip_id.to_string(),
            trace_height: 1 << opening.degree_bits,
        });
    }
    for col in &proof.columns {
        for opening in &col.proof.chip_openings {
            chips.push(ChipSummary {
                name: opening.chip_id.to_string(),
                trace_height: 1 << opening.degree_bits,
            });
        }
    }
    for opening in &proof.root.chip_openings {
        chips.push(ChipSummary {
            name: opening.chip_id.to_string(),
            trace_height: 1 << opening.degree_bits,
        });
    }
    chips
}
