//! AIR constraints for the Precompile opcode.
//!
//! When `op_precompile = 1`, the row reuses the Hash opcode's Poseidon
//! permutation columns. The I/O commitment is computed as:
//!
//!   `io_commitment = Poseidon(PRECOMPILE_DOMAIN_TAG, precompile_id, ...)`
//!
//! The commitment digest (`hash_perm_output[0..W]`) is bound to the written
//! slot, and `(precompile_id, hash_perm_output[0..8])` is sent on the
//! PRECOMPILE bus.

use p3_air::AirBuilder;

use super::super::columns::{ExecutionCols, MAX_SLOTS};
use tabula_gadgets::integer::expr_from_u32;

/// Domain separator for precompile I/O commitments.
///
/// Distinct from `HASH_INSTRUCTION_DOMAIN_TAG` (0x20) to prevent cross-domain
/// hash collisions.
pub const PRECOMPILE_DOMAIN_TAG: u32 = 0x30;

/// Constrain the Precompile opcode.
///
/// When `op_precompile = 1`:
/// - `hash_perm_input[0] = PRECOMPILE_DOMAIN_TAG`
/// - `hash_perm_input[1] = precompile_id`
/// - Result binding: `hash_perm_output[0..W]` written to the destination slot, not null
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_precompile<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_precompile.clone().into();

    // Domain tag check
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[0].clone().into()
                - expr_from_u32::<AB>(PRECOMPILE_DOMAIN_TAG)),
    );

    // Precompile ID consistency
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[1].clone().into() - local.precompile_id.clone().into()),
    );

    // Result binding: hash_perm_output[0..W] -> written slot, not null
    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].clone().into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone()
                    * (local.slots[s][i].clone().into() - local.hash_perm_output[i].clone().into()),
            );
        }
        // Written slot must not be null
        builder.assert_zero(slot_gate * local.slot_is_null[s].clone().into());
    }
}
