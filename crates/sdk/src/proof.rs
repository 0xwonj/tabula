#[cfg(feature = "prove")]
use tabula_runtime::ProofSummary;

use tabula_artifact::Statement;
use tabula_machine::TabulaProof;

/// In-memory proof bundle produced by the SDK.
pub struct Proof {
    pub(crate) proof: TabulaProof,
    pub(crate) statement: Statement,
    #[cfg(feature = "prove")]
    pub(crate) summary: ProofSummary,
}

impl Proof {
    #[cfg(feature = "prove")]
    pub(crate) fn from_prove_result(result: tabula_runtime::ProveResult) -> Self {
        Self {
            proof: result.proof,
            statement: result.statement,
            summary: result.summary,
        }
    }

    /// The canonical statement bound into the proof transcript.
    pub fn statement(&self) -> &Statement {
        &self.statement
    }

    /// Per-chip proof summary.
    #[cfg(feature = "prove")]
    pub fn summary(&self) -> &ProofSummary {
        &self.summary
    }
}

impl std::fmt::Debug for Proof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Proof");
        debug.field("statement", &self.statement);
        #[cfg(feature = "prove")]
        debug.field("summary", &self.summary);
        debug.finish_non_exhaustive()
    }
}
