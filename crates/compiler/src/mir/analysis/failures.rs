use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_ir as ir;

use super::super::model::{CapabilityId, Op};
use super::FailureSummary;

pub(super) fn update_failure_summary(
    op: &Op,
    capabilities: &BTreeMap<CapabilityId, &ir::CapabilityDescriptor>,
    summary: &mut FailureSummary,
) -> Result<(), TabulaError> {
    match op {
        Op::DivMod { .. }
        | Op::Assert { .. }
        | Op::AssertRelation { .. }
        | Op::EvalRelation { .. } => {
            summary.semantic_may_fail = true;
        }
        Op::CallCapability { capability, .. } => {
            let descriptor = capabilities.get(capability).ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown capability ID {}", capability.0))
            })?;
            match descriptor.totality {
                ir::CapabilityTotality::Checked => summary.semantic_may_fail = true,
                ir::CapabilityTotality::Total => summary.host_contract_sensitive = true,
            }
        }
        _ => {}
    }
    Ok(())
}
