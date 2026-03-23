//! Hash constraints for the ExecutionChip.
//!
//! The canonical portable hash itself is proven in the dedicated IR-hash lane.
//! The execution lane only binds the relayed digest to the written destination slot.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};

/// Hash constraint: digest relay and destination-slot binding.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_hash<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_hash.into();

    let written_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.slot_written[s].into()).sum();
    builder.assert_zero(gate.clone() * (written_sum - AB::Expr::ONE));

    // Result binding: the written destination slot carries the relayed digest prefix.
    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone() * (local.slots[s][i].into() - local.hash_digest[i].into()),
            );
        }
        builder.assert_zero(slot_gate * local.slot_is_null[s].into());
    }
}
