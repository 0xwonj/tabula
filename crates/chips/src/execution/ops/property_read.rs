//! AIR constraints for the PropertyRead opcode.
//!
//! When `op_property_read = 1`, the row carries a structural query result
//! from committed column state. Three slots are written:
//! - val slot (identified by `property_val_sel`): the result value
//! - key slot (identified by `property_key_sel`): the satisfying key
//! - null slot (the remaining written slot): the is_null flag
//!
//! A `PROPERTY_READ` bus interaction sends the result to Tier 2 for
//! verification against the column commitment.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use super::super::columns::{ExecutionCols, MAX_SLOTS};

/// Constrain the PropertyRead opcode.
///
/// When `op_property_read = 1`:
/// - `property_val_sel` is one-hot (exactly one slot receives the value)
/// - `property_key_sel` is one-hot (exactly one slot receives the key)
/// - val_sel and key_sel don't overlap
/// - Both selectors point to written slots
/// - Val slot value = property_result_val, not null
/// - Key slot value = property_result_key, not null
/// - Null slot value = [is_null, 0, 0], not null (it's a boolean value)
/// - property_result_is_null is boolean
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_property_read<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.op_property_read.clone().into();

    // property_result_is_null must be boolean
    builder.assert_zero(
        is_real.clone()
            * local.op_property_read.clone().into()
            * local.property_result_is_null.clone().into()
            * (AB::Expr::ONE - local.property_result_is_null.clone().into()),
    );

    // property_val_sel: boolean per element, one-hot sum = op_property_read
    let mut val_sel_sum = AB::Expr::ZERO;
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            is_real.clone()
                * local.property_val_sel[s].clone().into()
                * (AB::Expr::ONE - local.property_val_sel[s].clone().into()),
        );
        val_sel_sum += local.property_val_sel[s].clone().into();
    }
    builder.assert_zero(is_real.clone() * (val_sel_sum - local.op_property_read.clone().into()));

    // property_key_sel: boolean per element, one-hot sum = op_property_read
    let mut key_sel_sum = AB::Expr::ZERO;
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            is_real.clone()
                * local.property_key_sel[s].clone().into()
                * (AB::Expr::ONE - local.property_key_sel[s].clone().into()),
        );
        key_sel_sum += local.property_key_sel[s].clone().into();
    }
    builder.assert_zero(is_real.clone() * (key_sel_sum - local.op_property_read.clone().into()));

    // Non-overlap: val_sel[s] * key_sel[s] = 0 for all s
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            gate.clone()
                * local.property_val_sel[s].clone().into()
                * local.property_key_sel[s].clone().into(),
        );
    }

    // Selectors must point to written slots
    for s in 0..MAX_SLOTS {
        // val_sel[s] → slot_written[s]
        builder.assert_zero(
            gate.clone()
                * local.property_val_sel[s].clone().into()
                * (AB::Expr::ONE - local.slot_written[s].clone().into()),
        );
        // key_sel[s] → slot_written[s]
        builder.assert_zero(
            gate.clone()
                * local.property_key_sel[s].clone().into()
                * (AB::Expr::ONE - local.slot_written[s].clone().into()),
        );
    }

    // Val slot binding: slots[s] = property_result_val, not null
    for s in 0..MAX_SLOTS {
        let val_gate: AB::Expr = gate.clone() * local.property_val_sel[s].clone().into();
        for i in 0..W {
            builder.assert_zero(
                val_gate.clone()
                    * (local.slots[s][i].clone().into()
                        - local.property_result_val[i].clone().into()),
            );
        }
        builder.assert_zero(val_gate * local.slot_is_null[s].clone().into());
    }

    // Key slot binding: slots[s] = property_result_key, not null
    for s in 0..MAX_SLOTS {
        let key_gate: AB::Expr = gate.clone() * local.property_key_sel[s].clone().into();
        for i in 0..W {
            builder.assert_zero(
                key_gate.clone()
                    * (local.slots[s][i].clone().into()
                        - local.property_result_key[i].clone().into()),
            );
        }
        builder.assert_zero(key_gate * local.slot_is_null[s].clone().into());
    }

    // Null slot binding: the written slot that is neither val nor key.
    // Its value must be [is_null, 0, 0, ...], not null.
    for s in 0..MAX_SLOTS {
        let null_gate: AB::Expr = gate.clone()
            * local.slot_written[s].clone().into()
            * (AB::Expr::ONE - local.property_val_sel[s].clone().into())
            * (AB::Expr::ONE - local.property_key_sel[s].clone().into());
        // slots[s][0] = property_result_is_null
        builder.assert_zero(
            null_gate.clone()
                * (local.slots[s][0].clone().into() - local.property_result_is_null.clone().into()),
        );
        // slots[s][1..] = 0
        for i in 1..W {
            builder.assert_zero(null_gate.clone() * local.slots[s][i].clone().into());
        }
        builder.assert_zero(null_gate * local.slot_is_null[s].clone().into());
    }

    // No access clock increment: is_access = 0 is already enforced by
    // is_access = op_read + op_write (PropertyRead is neither).
}
