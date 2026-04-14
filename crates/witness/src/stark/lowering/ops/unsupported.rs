//! Deferred native-proof features that are still intentionally unsupported.

use tabula_core::error::TabulaError;
use tabula_ir as ir;

use super::super::context::LoweringCx;

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn reject_deferred_proof_feature(
        &self,
        op_index: usize,
        op: &ir::Op,
    ) -> Result<(), TabulaError> {
        Err(TabulaError::ProofError {
            phase: "next_trace_lowering",
            detail: format!(
                "entry {} ('{}') tx={} op {} ({op:?}) is outside the current native proving subset: this feature is intentionally fail-closed during the unary native path",
                self.entry.id.0, self.entry.symbol, self.tx_index, op_index,
            ),
        })
    }
}
