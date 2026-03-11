//! Mul operation: columns + constraints for the ExecutionChip.

use p3_air::AirBuilder;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};
use tabula_gadgets::integer::{LimbHalves, SHIFT_30_U32, expr_from_u32};

/// Mul carry columns: cross-product carry chain witnesses.
///
/// Columns: 5 (c0 + LimbHalves(2) + c1_lo + c1_hi).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct MulCarry<T> {
    /// Carry from limb0 to limb1 (c0 ∈ [0, 2^30)).
    pub c0: T,
    /// Half-decomposition of c0 for range check.
    pub c0_halves: LimbHalves<T>,
    /// Low part of carry from limb1 to limb2 (c1_lo ∈ [0, 2^16)).
    pub c1_lo: T,
    /// High part of carry from limb1 to limb2 (c1_hi ∈ [0, 2^15)).
    pub c1_hi: T,
}

/// Arith(Mul) constraint: integer multiply via limb cross-product carry chain.
///
/// For each written slot s:
///   a0*b0 = r0 + mul.c0 * 2^30
///   a0*b1 + a1*b0 + mul.c0 = r1 + (mul.c1_lo + mul.c1_hi * 2^16) * 2^30
///   a0*b2 + a1*b1 + a2*b0 + mul.c1_lo + mul.c1_hi * 2^16 = r2
///   a1*b2 + a2*b1 = 0  (no overflow)
///   a2*b2 = 0           (no overflow)
pub(crate) fn constrain_arith_mul<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    const {
        assert!(
            W >= 3,
            "Mul constraints require W >= 3 (3-limb value encoding)"
        );
    };

    let op_mul: AB::Expr = local.op_arith.clone().into() * local.arith_is_mul.clone().into();
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_16: AB::Expr = expr_from_u32::<AB>(1 << 16);

    let a0: AB::Expr = local.src1_val[0].clone().into();
    let a1: AB::Expr = local.src1_val[1].clone().into();
    let a2: AB::Expr = local.src1_val[2].clone().into();
    let b0: AB::Expr = local.src2_val[0].clone().into();
    let b1: AB::Expr = local.src2_val[1].clone().into();
    let b2: AB::Expr = local.src2_val[2].clone().into();

    let c0: AB::Expr = local.mul.c0.clone().into();
    let c1: AB::Expr = local.mul.c1_lo.clone().into() + local.mul.c1_hi.clone().into() * shift_16;

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_mul.clone() * local.slot_written[s].clone().into();

        let r0: AB::Expr = local.slots[s][0].clone().into();
        let r1: AB::Expr = local.slots[s][1].clone().into();
        let r2: AB::Expr = local.slots[s][2].clone().into();

        // (1) a0*b0 = r0 + c0 * 2^30
        builder.assert_zero(
            gate.clone() * (a0.clone() * b0.clone() - r0 - c0.clone() * shift_30.clone()),
        );

        // (2) a0*b1 + a1*b0 + c0 = r1 + c1 * 2^30
        builder.assert_zero(
            gate.clone()
                * (a0.clone() * b1.clone() + a1.clone() * b0.clone() + c0.clone()
                    - r1
                    - c1.clone() * shift_30.clone()),
        );

        // (3) a0*b2 + a1*b1 + a2*b0 + c1 = r2
        builder.assert_zero(
            gate.clone()
                * (a0.clone() * b2.clone()
                    + a1.clone() * b1.clone()
                    + a2.clone() * b0.clone()
                    + c1.clone()
                    - r2),
        );
    }

    // (4)(5) No overflow constraints (not per-slot)
    let gate_noslot: AB::Expr = is_real * op_mul;
    builder.assert_zero(gate_noslot.clone() * (a1.clone() * b2.clone() + a2.clone() * b1.clone()));
    builder.assert_zero(gate_noslot * (a2 * b2));
}
