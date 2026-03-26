//! AIR constraints for the Capability-call opcode.
//!
//! The generic execution lane proves:
//! - call-site metadata (`instruction_index`, `capability_transcript_id`, input/output counts)
//! - actual program-visible destination-slot writes
//! - canonical transcript digest relay to the shared CAPABILITY_TRANSCRIPT bus
//!
//! The digest itself is proven in the separate transcript lane.

use p3_air::AirBuilder;

use super::super::columns::{ExecutionCols, MAX_SLOTS};

/// Constrain the Capability-call opcode.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_capability_call<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_capability_call.into();

    // Output count must exactly match the number of written destination slots.
    let written_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.slot_written[s].into()).sum();
    builder.assert_zero(gate.clone() * (written_sum - local.capability_output_count.into()));

    // All program-visible capability outputs are concrete, not null.
    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].into();
        builder.assert_zero(slot_gate * local.slot_is_null[s].into());
    }
}
