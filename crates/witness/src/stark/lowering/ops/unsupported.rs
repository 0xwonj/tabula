//! Deferred native-proof features that are still intentionally unsupported.

use tabula_core::error::TabulaError;

use super::super::context::LoweringCx;

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn reject_deferred_proof_feature(&self, op_index: usize) -> Result<(), TabulaError> {
        Err(TabulaError::ProofError {
            phase: "next_trace_lowering",
            detail: format!(
                "op {op_index} in tx={} uses a deferred proof feature that is not part of the core-first native proving cutover",
                self.tx_index
            ),
        })
    }
}
