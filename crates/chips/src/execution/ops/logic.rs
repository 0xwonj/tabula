//! Boolean logic constraints (Not, And, Or) for the ExecutionChip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};

/// Not constraint: boolean negation.
///
/// For each written slot s:
///   slots[s][0] = 1 - src1_val[0]
///   slots[s][i] = 0   for i > 0
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_not<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_not: AB::Expr = local.op_not.clone().into();

    // Boolean input: src1_val[0] ∈ {0, 1}
    let src1: AB::Expr = local.src1_val[0].clone().into();
    builder.assert_zero(is_real.clone() * op_not.clone() * src1.clone() * (src1 - AB::Expr::ONE));

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_not.clone() * local.slot_written[s].clone().into();

        // Limb 0: dst = 1 - src1
        let expected: AB::Expr = AB::Expr::ONE - local.src1_val[0].clone().into();
        builder.assert_zero(gate.clone() * (local.slots[s][0].clone().into() - expected));

        // Higher limbs must be zero
        for i in 1..W {
            builder.assert_zero(gate.clone() * local.slots[s][i].clone().into());
        }
    }
}

/// And constraint: boolean conjunction.
///
/// For each written slot s:
///   slots[s][0] = src1_val[0] * src2_val[0]
///   slots[s][i] = 0   for i > 0
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_and<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_and: AB::Expr = local.op_and.clone().into();

    // Boolean inputs: src1_val[0], src2_val[0] ∈ {0, 1}
    let src1: AB::Expr = local.src1_val[0].clone().into();
    builder.assert_zero(is_real.clone() * op_and.clone() * src1.clone() * (src1 - AB::Expr::ONE));
    let src2: AB::Expr = local.src2_val[0].clone().into();
    builder.assert_zero(is_real.clone() * op_and.clone() * src2.clone() * (src2 - AB::Expr::ONE));

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_and.clone() * local.slot_written[s].clone().into();

        // Limb 0: dst = src1 * src2
        let expected: AB::Expr =
            local.src1_val[0].clone().into() * local.src2_val[0].clone().into();
        builder.assert_zero(gate.clone() * (local.slots[s][0].clone().into() - expected));

        // Higher limbs must be zero
        for i in 1..W {
            builder.assert_zero(gate.clone() * local.slots[s][i].clone().into());
        }
    }
}

/// Or constraint: boolean disjunction.
///
/// For each written slot s:
///   slots[s][0] = src1_val[0] + src2_val[0] - src1_val[0] * src2_val[0]
///   slots[s][i] = 0   for i > 0
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_or<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_or: AB::Expr = local.op_or.clone().into();

    // Boolean inputs: src1_val[0], src2_val[0] ∈ {0, 1}
    let src1: AB::Expr = local.src1_val[0].clone().into();
    builder.assert_zero(is_real.clone() * op_or.clone() * src1.clone() * (src1 - AB::Expr::ONE));
    let src2: AB::Expr = local.src2_val[0].clone().into();
    builder.assert_zero(is_real.clone() * op_or.clone() * src2.clone() * (src2 - AB::Expr::ONE));

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * op_or.clone() * local.slot_written[s].clone().into();

        // Limb 0: dst = src1 + src2 - src1 * src2
        let s1: AB::Expr = local.src1_val[0].clone().into();
        let s2: AB::Expr = local.src2_val[0].clone().into();
        let expected: AB::Expr = s1.clone() + s2.clone() - s1 * s2;
        builder.assert_zero(gate.clone() * (local.slots[s][0].clone().into() - expected));

        // Higher limbs must be zero
        for i in 1..W {
            builder.assert_zero(gate.clone() * local.slots[s][i].clone().into());
        }
    }
}
