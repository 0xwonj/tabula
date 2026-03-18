//! Hash constraints for the ExecutionChip.

use p3_air::AirBuilder;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};
use tabula_gadgets::integer::expr_from_u32;

use crate::execution::air::{HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT};

/// Hash constraint: input composition and result binding.
///
/// Enforces:
/// - `hash_perm_input[2..2+W] = src1_val[0..W]`
/// - `hash_perm_input[2+W..2+2W] = src2_val[0..W]`
/// - `hash_perm_input[8..16] = 0` (capacity zero for fresh sponge)
/// - Result: `perm_output[0..W] -> dst slot`, not null
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_hash<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.op_hash.into();

    // Domain tag: perm_input[0] = HASH_INSTRUCTION_DOMAIN_TAG
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[0].into() - expr_from_u32::<AB>(HASH_INSTRUCTION_DOMAIN_TAG)),
    );

    // Input count: perm_input[1] = 2 (Hash always takes 2 values)
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[1].into() - expr_from_u32::<AB>(HASH_INSTRUCTION_INPUT_COUNT)),
    );

    // Input composition: perm_input[2..2+W] = src1_val
    for i in 0..W {
        builder.assert_zero(
            gate.clone() * (local.hash_perm_input[2 + i].into() - local.src1_val[i].into()),
        );
    }

    // Input composition: perm_input[2+W..2+2W] = src2_val
    for i in 0..W {
        builder.assert_zero(
            gate.clone() * (local.hash_perm_input[2 + W + i].into() - local.src2_val[i].into()),
        );
    }

    // Capacity zero: perm_input[8..16] = 0
    for i in 8..16 {
        builder.assert_zero(gate.clone() * local.hash_perm_input[i].into());
    }

    // Result binding: perm_output[0..W] -> destination slot
    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone() * (local.slots[s][i].into() - local.hash_perm_output[i].into()),
            );
        }
        // Not null
        builder.assert_zero(slot_gate * local.slot_is_null[s].into());
    }
}
