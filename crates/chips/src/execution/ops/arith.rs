//! Add/Sub constraints for the ExecutionChip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};
use tabula_gadgets::integer::{SHIFT_30_U32, expr_from_u32};

/// Arith(Add) constraint: integer add via limb carry chain.
///
/// For each written slot s:
///   slots[s][0] + carry0 * 2^30 = src1_val[0] + src2_val[0]
///   slots[s][1] + carry1 * 2^30 = src1_val[1] + src2_val[1] + carry0
///   slots[s][2]                  = src1_val[2] + src2_val[2] + carry1
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_arith_add<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    const {
        assert!(
            W >= 3,
            "Add constraints require W >= 3 (3-limb value encoding)"
        );
    };

    let op_add: AB::Expr = local.op_arith.into()
        * (AB::Expr::ONE - local.arith_is_sub.into())
        * (AB::Expr::ONE - local.arith_is_mul.into());

    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * op_add.clone() * local.slot_written[s].into();

        // Limb 0: slots[s][0] + carry0 * 2^30 = src1[0] + src2[0]
        let lhs0: AB::Expr = local.slots[s][0].into() + local.carry0.into() * shift_30.clone();
        let rhs0: AB::Expr = local.src1_val[0].into() + local.src2_val[0].into();
        builder.assert_zero(gate.clone() * (lhs0 - rhs0));

        // Limb 1: slots[s][1] + carry1 * 2^30 = src1[1] + src2[1] + carry0
        let lhs1: AB::Expr = local.slots[s][1].into() + local.carry1.into() * shift_30.clone();
        let rhs1: AB::Expr =
            local.src1_val[1].into() + local.src2_val[1].into() + local.carry0.into();
        builder.assert_zero(gate.clone() * (lhs1 - rhs1));

        // Limb 2: slots[s][2] = src1[2] + src2[2] + carry1
        let lhs2: AB::Expr = local.slots[s][2].into();
        let rhs2: AB::Expr =
            local.src1_val[2].into() + local.src2_val[2].into() + local.carry1.into();
        builder.assert_zero(gate * (lhs2 - rhs2));
    }
}

/// Arith(Sub) constraint: integer sub via limb borrow chain.
///
/// For each written slot s:
///   slots[s][0] = src1_val[0] - src2_val[0] + carry0 * 2^30
///   slots[s][1] = src1_val[1] - src2_val[1] - carry0 + carry1 * 2^30
///   slots[s][2] = src1_val[2] - src2_val[2] - carry1
///
/// Here carry0/carry1 are borrow flags.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_arith_sub<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    const {
        assert!(
            W >= 3,
            "Sub constraints require W >= 3 (3-limb value encoding)"
        );
    };

    let op_sub: AB::Expr = local.op_arith.into() * local.arith_is_sub.into();
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * op_sub.clone() * local.slot_written[s].into();

        // Limb 0: slots[s][0] = src1[0] - src2[0] + carry0 * 2^30
        let expected0: AB::Expr = local.src1_val[0].into() - local.src2_val[0].into()
            + local.carry0.into() * shift_30.clone();
        builder.assert_zero(gate.clone() * (local.slots[s][0].into() - expected0));

        // Limb 1: slots[s][1] = src1[1] - src2[1] - carry0 + carry1 * 2^30
        let expected1: AB::Expr =
            local.src1_val[1].into() - local.src2_val[1].into() - local.carry0.into()
                + local.carry1.into() * shift_30.clone();
        builder.assert_zero(gate.clone() * (local.slots[s][1].into() - expected1));

        // Limb 2: slots[s][2] = src1[2] - src2[2] - carry1
        let expected2: AB::Expr =
            local.src1_val[2].into() - local.src2_val[2].into() - local.carry1.into();
        builder.assert_zero(gate * (local.slots[s][2].into() - expected2));
    }
}
