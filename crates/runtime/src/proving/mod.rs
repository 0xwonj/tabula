mod artifacts;
mod prepare;
mod statement;
mod traces;

use serde::{Deserialize, Serialize};
use tabula_machine::TabulaProof;

pub(crate) use artifacts::prepare_witness_artifacts;
pub(crate) use statement::build_execution_statement;
pub use statement::digest_to_hex;
pub(crate) use traces::build_traces;

/// Inputs for the proving pipeline.
///
/// Bundles the execution result with the original state and batch files
/// needed for witness generation and column state reconstruction.
pub struct ProveInput<'a> {
    /// Pre-execution state (for building `old_column_states`).
    pub state: &'a tabula_artifact::StateSnapshot,
    /// Transaction batch (for witness store preparation).
    pub batch: &'a tabula_artifact::TransactionBatch,
    /// Executed batch result (from [`run_batch`](crate::run_batch)).
    pub executed: &'a crate::execute::ExecutedBatch,
}

/// Result of STARK proof generation.
pub struct ProveResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Canonical execution statement bound into the proof transcript.
    pub statement: tabula_artifact::ExecutionStatement,
    /// Summary of chips contributing to the proof.
    pub summary: ProofSummary,
}

/// Result of prove + verify.
pub struct VerifiedResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Canonical execution statement bound into the proof transcript.
    pub statement: tabula_artifact::ExecutionStatement,
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
