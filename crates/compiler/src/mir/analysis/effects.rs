use super::super::model::Op;
use super::EffectSummary;

pub(super) fn update_effect_summary(op: &Op, summary: &mut EffectSummary) {
    match op {
        Op::ReadState { .. } => {
            summary.world.state_read = true;
        }
        Op::WriteState { .. } => {
            summary.world.state_write = true;
        }
        Op::DeleteState { .. } => {
            summary.world.state_delete = true;
        }
        Op::ReadStateProperty { .. } => {
            summary.proof.state_property_read = true;
        }
        Op::AssertRelation { .. } | Op::EvalRelation { .. } => {
            summary.proof.relation_use = true;
        }
        Op::CallCapability { .. } => {
            summary.proof.capability_call = true;
        }
        Op::EmitEvent { .. } => {
            summary.world.emit_event = true;
        }
        _ => {}
    }
}
