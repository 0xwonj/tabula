//! Cmp constraints for the ExecutionChip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::air::chips::execution::columns::{ExecutionCols, MAX_SLOTS};
use crate::air::gadgets::integer::{SHIFT_30_U32, expr_from_u32};

/// Cmp constraint: comparison operations (Eq, Ne, Lt, Lte, Gt, Gte).
///
/// Sub-selector one-hot, equality via IsZero on combined diff,
/// ordering via StrictIneq, result binding per sub-operation.
pub(crate) fn constrain_cmp<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    if W < 3 {
        return;
    }

    let op_cmp: AB::Expr = local.op_cmp.clone().into();
    let gate: AB::Expr = is_real.clone() * op_cmp.clone();

    // Sub-selector exactly-one when op_cmp
    let cmp_sum: AB::Expr = local.cmp_is_eq.clone().into()
        + local.cmp_is_ne.clone().into()
        + local.cmp_is_lt.clone().into()
        + local.cmp_is_lte.clone().into()
        + local.cmp_is_gt.clone().into()
        + local.cmp_is_gte.clone().into();
    builder.assert_zero(gate.clone() * (cmp_sum - AB::Expr::ONE));

    // lt and eq witnesses must be mutually exclusive
    builder.assert_zero(
        gate.clone() * local.cmp_lt_witness.clone().into() * local.cmp_eq_witness.clone().into(),
    );

    // Sub-selectors must be zero when not op_cmp
    let not_cmp: AB::Expr = AB::Expr::ONE - op_cmp;
    let not_cmp_gate: AB::Expr = is_real.clone() * not_cmp;
    builder.assert_zero(not_cmp_gate.clone() * local.cmp_is_eq.clone().into());
    builder.assert_zero(not_cmp_gate.clone() * local.cmp_is_ne.clone().into());
    builder.assert_zero(not_cmp_gate.clone() * local.cmp_is_lt.clone().into());
    builder.assert_zero(not_cmp_gate.clone() * local.cmp_is_lte.clone().into());
    builder.assert_zero(not_cmp_gate.clone() * local.cmp_is_gt.clone().into());
    builder.assert_zero(not_cmp_gate * local.cmp_is_gte.clone().into());

    // Equality detection: per-limb IsZero to avoid field reconstruction collision.
    let limb0_diff: AB::Expr = local.src1_val[0].clone().into() - local.src2_val[0].clone().into();
    let limb1_diff: AB::Expr = local.src1_val[1].clone().into() - local.src2_val[1].clone().into();
    let limb2_diff: AB::Expr = local.src1_val[2].clone().into() - local.src2_val[2].clone().into();

    // Gated IsZero on each limb diff
    builder.assert_bool(local.cmp_eq_limb0_iz.is_zero.clone());
    builder.assert_zero(
        gate.clone() * limb0_diff.clone() * local.cmp_eq_limb0_iz.is_zero.clone().into(),
    );
    let not_zero0: AB::Expr = AB::Expr::ONE - local.cmp_eq_limb0_iz.is_zero.clone().into();
    let has_inv0: AB::Expr = AB::Expr::ONE - limb0_diff * local.cmp_eq_limb0_iz.inv.clone().into();
    builder.assert_zero(gate.clone() * not_zero0 * has_inv0);

    builder.assert_bool(local.cmp_eq_limb1_iz.is_zero.clone());
    builder.assert_zero(
        gate.clone() * limb1_diff.clone() * local.cmp_eq_limb1_iz.is_zero.clone().into(),
    );
    let not_zero1: AB::Expr = AB::Expr::ONE - local.cmp_eq_limb1_iz.is_zero.clone().into();
    let has_inv1: AB::Expr = AB::Expr::ONE - limb1_diff * local.cmp_eq_limb1_iz.inv.clone().into();
    builder.assert_zero(gate.clone() * not_zero1 * has_inv1);

    builder.assert_bool(local.cmp_eq_limb2_iz.is_zero.clone());
    builder.assert_zero(
        gate.clone() * limb2_diff.clone() * local.cmp_eq_limb2_iz.is_zero.clone().into(),
    );
    let not_zero2: AB::Expr = AB::Expr::ONE - local.cmp_eq_limb2_iz.is_zero.clone().into();
    let has_inv2: AB::Expr = AB::Expr::ONE - limb2_diff * local.cmp_eq_limb2_iz.inv.clone().into();
    builder.assert_zero(gate.clone() * not_zero2 * has_inv2);

    // Equality = all three limbs equal: cmp_eq_witness = iz0 * iz1 * iz2
    let all_equal: AB::Expr = local.cmp_eq_limb0_iz.is_zero.clone().into()
        * local.cmp_eq_limb1_iz.is_zero.clone().into()
        * local.cmp_eq_limb2_iz.is_zero.clone().into();
    builder.assert_zero(gate.clone() * (local.cmp_eq_witness.clone().into() - all_equal));

    // Ordering proof: borrow-chain per-limb StrictIneq
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    let lt: AB::Expr = local.cmp_lt_witness.clone().into();
    let not_eq: AB::Expr = AB::Expr::ONE - local.cmp_eq_witness.clone().into();
    let not_lt: AB::Expr = AB::Expr::ONE - local.cmp_lt_witness.clone().into();

    // Borrow booleans
    builder.assert_zero(gate.clone() * not_eq.clone() * local.cmp_ineq.borrow0.clone().into()
        * (AB::Expr::ONE - local.cmp_ineq.borrow0.clone().into()));
    builder.assert_zero(gate.clone() * not_eq.clone() * local.cmp_ineq.borrow1.clone().into()
        * (AB::Expr::ONE - local.cmp_ineq.borrow1.clone().into()));

    // When lt=1: s1 < s2, so (a=s1, b=s2)
    let gate_lt: AB::Expr = gate.clone() * not_eq.clone() * lt.clone();
    // diff0 = s2[0] - s1[0] - 1 + borrow0 * 2^30
    builder.assert_zero(gate_lt.clone() * (local.cmp_ineq.diff0.clone().into()
        - (local.src2_val[0].clone().into() - local.src1_val[0].clone().into() - AB::Expr::ONE
            + local.cmp_ineq.borrow0.clone().into() * shift_30.clone())));
    // diff1 = s2[1] - s1[1] - borrow0 + borrow1 * 2^30
    builder.assert_zero(gate_lt.clone() * (local.cmp_ineq.diff1.clone().into()
        - (local.src2_val[1].clone().into() - local.src1_val[1].clone().into()
            - local.cmp_ineq.borrow0.clone().into()
            + local.cmp_ineq.borrow1.clone().into() * shift_30.clone())));
    // diff2 = s2[2] - s1[2] - borrow1
    builder.assert_zero(gate_lt * (local.cmp_ineq.diff2.clone().into()
        - (local.src2_val[2].clone().into() - local.src1_val[2].clone().into()
            - local.cmp_ineq.borrow1.clone().into())));

    // When lt=0, eq=0: s1 > s2, so (a=s2, b=s1)
    let gate_gt: AB::Expr = gate.clone() * not_eq * not_lt;
    builder.assert_zero(gate_gt.clone() * (local.cmp_ineq.diff0.clone().into()
        - (local.src1_val[0].clone().into() - local.src2_val[0].clone().into() - AB::Expr::ONE
            + local.cmp_ineq.borrow0.clone().into() * shift_30.clone())));
    builder.assert_zero(gate_gt.clone() * (local.cmp_ineq.diff1.clone().into()
        - (local.src1_val[1].clone().into() - local.src2_val[1].clone().into()
            - local.cmp_ineq.borrow0.clone().into()
            + local.cmp_ineq.borrow1.clone().into() * shift_30)));
    builder.assert_zero(gate_gt * (local.cmp_ineq.diff2.clone().into()
        - (local.src1_val[2].clone().into() - local.src2_val[2].clone().into()
            - local.cmp_ineq.borrow1.clone().into())));

    // Result binding per sub-selector
    let eq_w: AB::Expr = local.cmp_eq_witness.clone().into();
    let lt_w: AB::Expr = local.cmp_lt_witness.clone().into();

    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].clone().into();
        let dst: AB::Expr = local.slots[s][0].clone().into();

        // Eq: dst = eq_witness
        builder.assert_zero(
            slot_gate.clone() * local.cmp_is_eq.clone().into() * (dst.clone() - eq_w.clone()),
        );
        // Ne: dst = 1 - eq_witness
        builder.assert_zero(
            slot_gate.clone()
                * local.cmp_is_ne.clone().into()
                * (dst.clone() - (AB::Expr::ONE - eq_w.clone())),
        );
        // Lt: dst = lt_witness
        builder.assert_zero(
            slot_gate.clone() * local.cmp_is_lt.clone().into() * (dst.clone() - lt_w.clone()),
        );
        // Lte: dst = lt_witness + eq_witness
        builder.assert_zero(
            slot_gate.clone()
                * local.cmp_is_lte.clone().into()
                * (dst.clone() - lt_w.clone() - eq_w.clone()),
        );
        // Gt: dst = 1 - lt_witness - eq_witness
        builder.assert_zero(
            slot_gate.clone()
                * local.cmp_is_gt.clone().into()
                * (dst.clone() - (AB::Expr::ONE - lt_w.clone() - eq_w.clone())),
        );
        // Gte: dst = 1 - lt_witness
        builder.assert_zero(
            slot_gate.clone()
                * local.cmp_is_gte.clone().into()
                * (dst - (AB::Expr::ONE - lt_w.clone())),
        );

        // Higher limbs zero for cmp result
        for i in 1..W {
            builder.assert_zero(slot_gate.clone() * local.slots[s][i].clone().into());
        }
        // Not null
        builder.assert_zero(slot_gate * local.slot_is_null[s].clone().into());
    }
}
