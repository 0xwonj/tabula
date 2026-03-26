use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_ir as ir;

use super::super::model::{CapabilityId, Op, ValueOp};
use super::PolicySummary;

pub(super) fn update_policy_summary(
    op: &Op,
    capabilities: &BTreeMap<CapabilityId, &ir::CapabilityDescriptor>,
    summary: &mut PolicySummary,
) -> Result<(), TabulaError> {
    match op {
        Op::BindValue {
            value: ValueOp::Hash { .. },
            ..
        } => {
            summary.uses_builtin_hash = true;
        }
        Op::CallCapability { capability, .. } => {
            let descriptor = capabilities.get(capability).ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown capability ID {}", capability.0))
            })?;
            match descriptor.query_policy {
                ir::CapabilityQueryPolicy::QuerySafe => {
                    summary.uses_query_safe_capability = true;
                }
                ir::CapabilityQueryPolicy::TxOnly => {
                    summary.uses_tx_only_capability = true;
                }
            }
            match descriptor.proof_visibility {
                ir::CapabilityProofVisibility::Journaled => {
                    summary.uses_journaled_capability = true;
                }
                ir::CapabilityProofVisibility::OpaqueRuntimeOnly => {
                    summary.uses_opaque_runtime_capability = true;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
