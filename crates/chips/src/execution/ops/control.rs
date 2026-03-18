//! Assert and Select constraints for the ExecutionChip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};

/// Assert constraint: condition value must be 1.
///
/// op_assert => src1_val[0] = 1
pub(crate) fn constrain_assert<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_assert.into();
    builder.assert_zero(gate * (local.src1_val[0].into() - AB::Expr::ONE));
}

/// Select constraint: conditional value selection.
///
/// For each written slot s:
///   slots[s][i] = cond * src1_val[i] + (1 - cond) * src2_val[i]
///
/// Simplified: slots[s][i] = src2_val[i] + cond * (src1_val[i] - src2_val[i])
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_select<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_select: AB::Expr = local.op_select.into();
    let cond: AB::Expr = local.cond_val.into();

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * op_select.clone() * local.slot_written[s].into();

        for i in 0..W {
            let expected: AB::Expr = local.src2_val[i].into()
                + cond.clone() * (local.src1_val[i].into() - local.src2_val[i].into());
            builder.assert_zero(gate.clone() * (local.slots[s][i].into() - expected));
        }

        // Select result is always non-null (Select is a value-level operation)
        builder.assert_zero(gate * local.slot_is_null[s].into());
    }
}
