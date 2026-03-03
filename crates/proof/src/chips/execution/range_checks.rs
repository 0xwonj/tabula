//! Range-check half-decomposition constraints for access_r, cmp, mul, divmod.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::gadgets::integer::{constrain_limb2_bits, expr_from_u32};

use super::columns::ExecutionCols;

/// Range-check half-decomposition constraints for access_r, cmp, mul, divmod.
pub(super) fn constrain_range_check_halves<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let is_real_for_cmp = is_real.clone();
    let is_real_for_mul_divmod = is_real.clone();
    let gate: AB::Expr = is_real * local.is_access.clone().into();

    // access_r limbs
    let r_l0_diff: AB::Expr = local.access_r.limbs.limb0.clone().into()
        - (local.access_r.l0_halves.lo.clone().into()
            + local.access_r.l0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(gate.clone() * r_l0_diff);

    let r_l1_diff: AB::Expr = local.access_r.limbs.limb1.clone().into()
        - (local.access_r.l1_halves.lo.clone().into()
            + local.access_r.l1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(gate * r_l1_diff);

    // access_r limb2 (4-bit boolean decomposition — no gating needed,
    // zero columns satisfy the constraint trivially)
    constrain_limb2_bits(
        builder,
        local.access_r.limbs.limb2.clone().into(),
        &local.access_r.limb2_bits,
    );

    // Cmp inequality diff halves (gated by op_cmp * (1 - cmp_eq_witness))
    let cmp_gate: AB::Expr = is_real_for_cmp
        * local.op_cmp.clone().into()
        * (AB::Expr::ONE - local.cmp.eq_witness.clone().into());

    let cmp_d0_diff: AB::Expr = local.cmp.ineq.diff0.clone().into()
        - (local.cmp.ineq_diff0_halves.lo.clone().into()
            + local.cmp.ineq_diff0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(cmp_gate.clone() * cmp_d0_diff);

    let cmp_d1_diff: AB::Expr = local.cmp.ineq.diff1.clone().into()
        - (local.cmp.ineq_diff1_halves.lo.clone().into()
            + local.cmp.ineq_diff1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(cmp_gate * cmp_d1_diff);

    // Cmp ineq diff2 (4-bit boolean decomposition)
    constrain_limb2_bits(
        builder,
        local.cmp.ineq.diff2.clone().into(),
        &local.cmp.ineq_diff2_bits,
    );

    // Mul carry half-decomposition (gated by op_arith * arith_is_mul)
    let mul_gate: AB::Expr = is_real_for_mul_divmod.clone()
        * local.op_arith.clone().into()
        * local.arith_is_mul.clone().into();
    let mul_c0_diff: AB::Expr = local.mul.c0.clone().into()
        - (local.mul.c0_halves.lo.clone().into()
            + local.mul.c0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(mul_gate * mul_c0_diff);

    // DivMod carry + remainder half-decomposition (gated by op_divmod)
    let divmod_gate: AB::Expr = is_real_for_mul_divmod * local.op_divmod.clone().into();
    let divmod_c0_diff: AB::Expr = local.divmod.c0.clone().into()
        - (local.divmod.c0_halves.lo.clone().into()
            + local.divmod.c0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate.clone() * divmod_c0_diff);

    let divmod_rd0_diff: AB::Expr = local.divmod.rem_ineq.diff0.clone().into()
        - (local.divmod.rem_diff0_halves.lo.clone().into()
            + local.divmod.rem_diff0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate.clone() * divmod_rd0_diff);

    let divmod_rd1_diff: AB::Expr = local.divmod.rem_ineq.diff1.clone().into()
        - (local.divmod.rem_diff1_halves.lo.clone().into()
            + local.divmod.rem_diff1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate * divmod_rd1_diff);

    // DivMod remainder ineq diff2 (4-bit boolean decomposition)
    constrain_limb2_bits(
        builder,
        local.divmod.rem_ineq.diff2.clone().into(),
        &local.divmod.rem_diff2_bits,
    );
}
