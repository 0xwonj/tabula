//! DivMod operation: columns + constraints for the ExecutionChip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use crate::execution::columns::{ExecutionCols, MAX_SLOTS};
use tabula_gadgets::integer::{
    IsZero, Limb2Bits, LimbHalves, SHIFT_30_U32, StrictIneq, expr_from_u32,
};

/// DivMod witness columns: quotient selector + carry chain + remainder bound.
///
/// Columns: 36 (q_sel[S] + c0 + LimbHalves(2) + c1_lo + c1_hi + StrictIneq(5) +
///           2×LimbHalves(4) + Limb2Bits(4) + IsZero(2)) where S=MAX_SLOTS.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct DivModWitness<T, const S: usize> {
    /// One-hot selector: which slot holds the quotient.
    /// Derived: `r_sel[s] = slot_written[s] - q_sel[s]` gives remainder slot.
    pub q_sel: [T; S],
    /// Carry from product limb0 (c0 ∈ [0, 2^30)).
    pub c0: T,
    /// Half-decomposition of c0 for range check.
    pub c0_halves: LimbHalves<T>,
    /// Low part of carry from product limb1 (c1_lo ∈ [0, 2^16)).
    pub c1_lo: T,
    /// High part of carry from product limb1 (c1_hi ∈ [0, 2^15)).
    pub c1_hi: T,
    /// StrictIneq gap for rem < rhs.
    pub rem_ineq: StrictIneq<T>,
    /// Range check halves for rem_ineq.diff0.
    pub rem_diff0_halves: LimbHalves<T>,
    /// Range check halves for rem_ineq.diff1.
    pub rem_diff1_halves: LimbHalves<T>,
    /// 4-bit boolean decomposition of rem_ineq.diff2.
    pub rem_diff2_bits: Limb2Bits<T>,
    /// IsZero for rhs ≠ 0 check.
    pub rhs_iz: IsZero<T>,
}

/// DivMod constraint: integer division + remainder.
///
/// Identity: `lhs = q * rhs + rem` where q = quotient, rem = remainder.
/// Carry chain for `q * rhs + rem` (same cross-product structure as Mul):
///   q0*d0 + rem0 = l0 + c0 * 2^30
///   q0*d1 + q1*d0 + rem1 + c0 = l1 + c1 * 2^30
///   q0*d2 + q1*d1 + q2*d0 + rem2 + c1 = l2
///   q1*d2 + q2*d1 = 0    (no overflow)
///   q2*d2 = 0             (no overflow)
/// Plus: rem < rhs (StrictIneq), rhs != 0 (IsZero).
///
/// Uses `divmod.q_sel` one-hot selector to identify the quotient slot.
/// Remainder slot is derived: `r_sel[s] = slot_written[s] - q_sel[s]`.
pub(crate) fn constrain_divmod<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    if W < 3 {
        return;
    }

    let gate: AB::Expr = is_real.clone() * local.op_divmod.clone().into();
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_16: AB::Expr = expr_from_u32::<AB>(1 << 16);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();

    // ── q_sel constraints ──
    // Boolean
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.divmod.q_sel[s].clone());
    }
    // Exactly-one when op_divmod
    let q_sel_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.divmod.q_sel[s].clone().into())
        .sum();
    builder.assert_zero(gate.clone() * (q_sel_sum - AB::Expr::ONE));
    // Subset: q_sel[s] only where slot_written[s]=1
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            gate.clone()
                * local.divmod.q_sel[s].clone().into()
                * (AB::Expr::ONE - local.slot_written[s].clone().into()),
        );
    }

    // ── Extract q and rem values via selectors ──
    let mut q_vals = Vec::with_capacity(W);
    let mut rem_vals = Vec::with_capacity(W);
    for i in 0..W {
        let q_i: AB::Expr = (0..MAX_SLOTS)
            .map(|s| local.divmod.q_sel[s].clone().into() * local.slots[s][i].clone().into())
            .sum();
        let rem_i: AB::Expr = (0..MAX_SLOTS)
            .map(|s| {
                let r_sel_s: AB::Expr =
                    local.slot_written[s].clone().into() - local.divmod.q_sel[s].clone().into();
                r_sel_s * local.slots[s][i].clone().into()
            })
            .sum();
        q_vals.push(q_i);
        rem_vals.push(rem_i);
    }

    // Extract null flags
    let q_null: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.divmod.q_sel[s].clone().into() * local.slot_is_null[s].clone().into())
        .sum();
    let rem_null: AB::Expr = (0..MAX_SLOTS)
        .map(|s| {
            let r_sel_s: AB::Expr =
                local.slot_written[s].clone().into() - local.divmod.q_sel[s].clone().into();
            r_sel_s * local.slot_is_null[s].clone().into()
        })
        .sum();

    // ── Division identity ──
    let l0: AB::Expr = local.src1_val[0].clone().into();
    let l1: AB::Expr = local.src1_val[1].clone().into();
    let l2: AB::Expr = local.src1_val[2].clone().into();

    let d0: AB::Expr = local.src2_val[0].clone().into();
    let d1: AB::Expr = local.src2_val[1].clone().into();
    let d2: AB::Expr = local.src2_val[2].clone().into();

    let c0: AB::Expr = local.divmod.c0.clone().into();
    let c1: AB::Expr =
        local.divmod.c1_lo.clone().into() + local.divmod.c1_hi.clone().into() * shift_16;

    // (1) q0*d0 + rem0 = l0 + c0 * 2^30
    builder.assert_zero(
        gate.clone()
            * (q_vals[0].clone() * d0.clone() + rem_vals[0].clone()
                - l0
                - c0.clone() * shift_30.clone()),
    );

    // (2) q0*d1 + q1*d0 + rem1 + c0 = l1 + c1 * 2^30
    builder.assert_zero(
        gate.clone()
            * (q_vals[0].clone() * d1.clone()
                + q_vals[1].clone() * d0.clone()
                + rem_vals[1].clone()
                + c0
                - l1
                - c1.clone() * shift_30.clone()),
    );

    // (3) q0*d2 + q1*d1 + q2*d0 + rem2 + c1 = l2
    builder.assert_zero(
        gate.clone()
            * (q_vals[0].clone() * d2.clone()
                + q_vals[1].clone() * d1.clone()
                + q_vals[2].clone() * d0.clone()
                + rem_vals[2].clone()
                + c1
                - l2),
    );

    // (4)(5) No overflow
    builder.assert_zero(
        gate.clone() * (q_vals[1].clone() * d2.clone() + q_vals[2].clone() * d1.clone()),
    );
    builder.assert_zero(gate.clone() * (q_vals[2].clone() * d2.clone()));

    // ── Remainder bound: rem < rhs via borrow-chain StrictIneq ──
    // Borrow booleans
    builder.assert_zero(
        gate.clone()
            * local.divmod.rem_ineq.borrow0.clone().into()
            * (AB::Expr::ONE - local.divmod.rem_ineq.borrow0.clone().into()),
    );
    builder.assert_zero(
        gate.clone()
            * local.divmod.rem_ineq.borrow1.clone().into()
            * (AB::Expr::ONE - local.divmod.rem_ineq.borrow1.clone().into()),
    );

    // diff0 = rhs[0] - rem[0] - 1 + borrow0 * 2^30
    builder.assert_zero(
        gate.clone()
            * (local.divmod.rem_ineq.diff0.clone().into()
                - (d0 - rem_vals[0].clone() - AB::Expr::ONE
                    + local.divmod.rem_ineq.borrow0.clone().into() * shift_30.clone())),
    );
    // diff1 = rhs[1] - rem[1] - borrow0 + borrow1 * 2^30
    builder.assert_zero(
        gate.clone()
            * (local.divmod.rem_ineq.diff1.clone().into()
                - (d1 - rem_vals[1].clone() - local.divmod.rem_ineq.borrow0.clone().into()
                    + local.divmod.rem_ineq.borrow1.clone().into() * shift_30.clone())),
    );
    // diff2 = rhs[2] - rem[2] - borrow1
    builder.assert_zero(
        gate.clone()
            * (local.divmod.rem_ineq.diff2.clone().into()
                - (d2.clone()
                    - rem_vals[2].clone()
                    - local.divmod.rem_ineq.borrow1.clone().into())),
    );

    let rhs_combined: AB::Expr = local.src2_val[0].clone().into()
        + local.src2_val[1].clone().into() * shift_30.clone()
        + local.src2_val[2].clone().into() * shift_60.clone();

    // ── Results not null ──
    builder.assert_zero(gate.clone() * q_null);
    builder.assert_zero(gate.clone() * rem_null);

    // ── Non-zero divisor: gated IsZero on combined rhs ──
    builder.assert_bool(local.divmod.rhs_iz.is_zero.clone());
    builder.assert_zero(gate.clone() * rhs_combined * local.divmod.rhs_iz.is_zero.clone().into());
    let not_zero: AB::Expr = AB::Expr::ONE - local.divmod.rhs_iz.is_zero.clone().into();
    let rhs_combined_for_inv: AB::Expr = local.src2_val[0].clone().into()
        + local.src2_val[1].clone().into() * shift_30
        + local.src2_val[2].clone().into() * shift_60;
    let has_inv: AB::Expr =
        AB::Expr::ONE - rhs_combined_for_inv * local.divmod.rhs_iz.inv.clone().into();
    builder.assert_zero(gate.clone() * not_zero * has_inv);
    // rhs must not be zero: is_zero flag must be 0
    builder.assert_zero(gate * local.divmod.rhs_iz.is_zero.clone().into());
}
