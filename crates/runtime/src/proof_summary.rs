//! Human-readable STARK proof summary for monitoring and diagnostics.

use serde::{Deserialize, Serialize};
use tabula_machine::TabulaProof;

/// Summary of one machine chip contribution in a STARK proof.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipSummary {
    /// Chip name.
    pub name: String,
    /// Trace height.
    pub trace_height: usize,
}

/// Serializable summary of the chips contributing to one proof.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSummary {
    /// Per-chip summaries.
    pub chips: Vec<ChipSummary>,
    /// Total number of chips.
    pub chip_count: usize,
}

impl ProofSummary {
    /// Extract one summary from a machine proof.
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
