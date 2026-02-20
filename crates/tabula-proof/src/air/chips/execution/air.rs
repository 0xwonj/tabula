//! ExecutionChip — AIR constraints for the instruction trace.
//!
//! One row per instruction. Constraints enforce:
//! 1. Boolean fields: all opcode selectors, is_access, access_is_write, slot_written, etc.
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Opcode exactly-one: sum of 12 opcode selectors = 1 when is_real
//! 4. `is_access` derived: is_access = op_read + op_write
//! 5. Clock recurrence: clk increments by is_access; first row clk=0
//! 6. Timestamp binding: is_access ⟹ tau = clk + 1
//! 7. Access log: access_is_write = op_write when is_access
//! 8. SSA slot carry: non-written slots carry forward to next row
//! 9. Arith sub-selectors: exactly one of {add, sub, mul} when op_arith
//! 10. Per-opcode semantics (delegated to `ops/`)
//! 11. Transaction index monotonicity
//! 12. Operand-to-slot linkage (delegated to `linkage`)

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::columns::borrow_cols;
use crate::air::gadgets::constrain_is_real_prefix;
use crate::air::gadgets::integer::{SHIFT_30_U32, constrain_limb2_bits, expr_from_u32};
use crate::air::interaction::{AirInteraction, InteractionKind};

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

/// Domain tag for the instruction-level Hash opcode.
///
/// Distinct from protocol-level domain tags (0x00=SSMC, 0x01=SMT, 0x10=leaf,
/// 0x11=tables, 0x12=cols) to prevent cross-protocol hash collisions.
pub const HASH_INSTRUCTION_DOMAIN_TAG: u32 = 0x20;

/// Number of input values for the Hash instruction (always 2: src1 and src2).
pub const HASH_INSTRUCTION_INPUT_COUNT: u32 = 2;

/// The ExecutionChip AIR, generic over value width.
#[derive(Debug)]
pub struct ExecutionChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for ExecutionChip<W> {
    fn width(&self) -> usize {
        execution_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for ExecutionChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &ExecutionCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &ExecutionCols<AB::Var, W> = borrow_cols(&next_row);

        let is_real: AB::Expr = local.is_real.clone().into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

        // ── Structural constraints ──
        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
        constrain_opcode_one_hot(builder, local, is_real.clone());
        constrain_is_access(builder, local, is_real.clone());
        constrain_clock(builder, local, next, both_real.clone());
        constrain_timestamp(builder, local, is_real.clone());
        constrain_access_log(builder, local, is_real.clone());
        constrain_arith_sub_selectors(builder, local, is_real.clone());
        constrain_slot_carry(builder, local, next, both_real.clone());
        constrain_first_row_init(builder, local);
        constrain_slot_written_count(builder, local, is_real.clone());

        // ── Per-opcode semantics (delegated to ops/) ──
        super::ops::arith::constrain_arith_add(builder, local, is_real.clone());
        super::ops::arith::constrain_arith_sub(builder, local, is_real.clone());
        super::ops::mul::constrain_arith_mul(builder, local, is_real.clone());
        constrain_arith_result_not_null(builder, local, is_real.clone());
        super::ops::divmod::constrain_divmod(builder, local, is_real.clone());
        super::ops::cmp::constrain_cmp(builder, local, is_real.clone());
        super::ops::control::constrain_assert(builder, local, is_real.clone());
        super::ops::control::constrain_select(builder, local, is_real.clone());
        super::ops::logic::constrain_not(builder, local, is_real.clone());
        super::ops::logic::constrain_and(builder, local, is_real.clone());
        super::ops::logic::constrain_or(builder, local, is_real.clone());
        super::ops::hash::constrain_hash(builder, local, is_real.clone());
        constrain_lookup(builder, local, is_real.clone());
        constrain_tx_index_monotonicity(builder, local, next, both_real);
        constrain_tau_decomposition(builder, local, is_real.clone());

        // ── Operand-to-slot linkage ──
        super::linkage::constrain_operand_selectors(builder, local, is_real.clone());
        super::linkage::constrain_operand_value_linkage(builder, local);
        super::linkage::constrain_write_operand(builder, local, is_real.clone());
        constrain_range_check_halves(builder, local, is_real.clone());
        super::linkage::constrain_read_destination(builder, local, is_real);

        // ── LogUp buses ──
        send_memory(builder, local);
        send_range_checks(builder, local);
        send_hash_permutation(builder, local);
        send_static_table_lookup(builder, local);
    }
}

// ── Structural constraint helpers ───────────────────────────────────────────

/// 1. Boolean constraints on all selector and flag columns.
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    // Opcode selectors
    builder.assert_bool(local.op_read.clone());
    builder.assert_bool(local.op_write.clone());
    builder.assert_bool(local.op_arith.clone());
    builder.assert_bool(local.op_divmod.clone());
    builder.assert_bool(local.op_cmp.clone());
    builder.assert_bool(local.op_not.clone());
    builder.assert_bool(local.op_and.clone());
    builder.assert_bool(local.op_or.clone());
    builder.assert_bool(local.op_assert.clone());
    builder.assert_bool(local.op_select.clone());
    builder.assert_bool(local.op_hash.clone());
    builder.assert_bool(local.op_lookup.clone());

    // Arith sub-selectors
    builder.assert_bool(local.arith_is_sub.clone());
    builder.assert_bool(local.arith_is_mul.clone());

    // Flags
    builder.assert_bool(local.is_access.clone());
    builder.assert_bool(local.access_is_write.clone());
    builder.assert_bool(local.access_is_null.clone());
    builder.assert_bool(local.cond_val.clone());
    builder.assert_bool(local.carry0.clone());
    builder.assert_bool(local.carry1.clone());

    // Cmp sub-selectors and witnesses
    builder.assert_bool(local.cmp_is_eq.clone());
    builder.assert_bool(local.cmp_is_ne.clone());
    builder.assert_bool(local.cmp_is_lt.clone());
    builder.assert_bool(local.cmp_is_lte.clone());
    builder.assert_bool(local.cmp_is_gt.clone());
    builder.assert_bool(local.cmp_is_gte.clone());
    builder.assert_bool(local.cmp_lt_witness.clone());
    builder.assert_bool(local.cmp_eq_witness.clone());

    // Per-slot flags
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.slot_is_null[s].clone());
        builder.assert_bool(local.slot_written[s].clone());
    }
}

/// 3. Opcode exactly-one: sum of 12 selectors = 1 when is_real.
fn constrain_opcode_one_hot<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let opcode_sum: AB::Expr = local.op_read.clone().into()
        + local.op_write.clone().into()
        + local.op_arith.clone().into()
        + local.op_divmod.clone().into()
        + local.op_cmp.clone().into()
        + local.op_not.clone().into()
        + local.op_and.clone().into()
        + local.op_or.clone().into()
        + local.op_assert.clone().into()
        + local.op_select.clone().into()
        + local.op_hash.clone().into()
        + local.op_lookup.clone().into();

    builder.assert_zero(is_real * (opcode_sum - AB::Expr::ONE));
}

/// 4. `is_access` derived: is_access = op_read + op_write.
fn constrain_is_access<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let derived: AB::Expr = local.op_read.clone().into() + local.op_write.clone().into();
    builder.assert_zero(is_real * (local.is_access.clone().into() - derived));
}

/// 5. Clock recurrence: next.clk = local.clk + local.is_access.
fn constrain_clock<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let clk_diff: AB::Expr =
        next.clk.clone().into() - local.clk.clone().into() - local.is_access.clone().into();
    builder.when_transition().assert_zero(both_real * clk_diff);
}

/// 6. Timestamp binding: is_access ⟹ tau = clk + 1.
fn constrain_timestamp<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let tau_expected: AB::Expr = local.clk.clone().into() + AB::Expr::ONE;
    let tau_diff: AB::Expr = local.tau.clone().into() - tau_expected;
    builder.assert_zero(is_real * local.is_access.clone().into() * tau_diff);
}

/// 7. Access log: access_is_write = op_write when is_access.
fn constrain_access_log<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.is_access.clone().into();
    builder
        .assert_zero(gate * (local.access_is_write.clone().into() - local.op_write.clone().into()));
}

/// 8. SSA slot carry: slots not written by the NEXT instruction carry forward.
fn constrain_slot_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    for s in 0..MAX_SLOTS {
        let not_written_next: AB::Expr = AB::Expr::ONE - next.slot_written[s].clone().into();
        let gate: AB::Expr = both_real.clone() * not_written_next;

        for i in 0..W {
            let diff: AB::Expr = next.slots[s][i].clone().into() - local.slots[s][i].clone().into();
            builder.when_transition().assert_zero(gate.clone() * diff);
        }

        let null_diff: AB::Expr =
            next.slot_is_null[s].clone().into() - local.slot_is_null[s].clone().into();
        builder.when_transition().assert_zero(gate * null_diff);
    }
}

/// 9. Arith sub-selectors: when op_arith, exactly one of {add, sub, mul}.
fn constrain_arith_sub_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_arith: AB::Expr = local.op_arith.clone().into();

    builder.assert_zero(
        is_real.clone()
            * op_arith.clone()
            * local.arith_is_sub.clone().into()
            * local.arith_is_mul.clone().into(),
    );

    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - op_arith.clone()) * local.arith_is_sub.clone().into(),
    );
    builder.assert_zero(is_real * (AB::Expr::ONE - op_arith) * local.arith_is_mul.clone().into());
}

/// 10a. First-row initialization: clk starts at zero, non-written slots start zeroed.
fn constrain_first_row_init<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.clk.clone());

    for s in 0..MAX_SLOTS {
        let not_written: AB::Expr = AB::Expr::ONE - local.slot_written[s].clone().into();
        for i in 0..W {
            builder
                .when_first_row()
                .when(local.is_real.clone())
                .assert_zero(not_written.clone() * local.slots[s][i].clone().into());
        }
        builder
            .when_first_row()
            .when(local.is_real.clone())
            .assert_zero(not_written * local.slot_is_null[s].clone().into());
    }
}

/// Slot written count constraint: total `slot_written` flags must match the opcode.
fn constrain_slot_written_count<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let written_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.slot_written[s].clone().into())
        .sum();

    let expected: AB::Expr =
        AB::Expr::ONE - local.op_write.clone().into() - local.op_assert.clone().into()
            + local.op_divmod.clone().into();

    builder.assert_zero(is_real * (written_sum - expected));
}

/// Arithmetic result null constraint: written slots must not be null.
fn constrain_arith_result_not_null<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_arith: AB::Expr = local.op_arith.clone().into();
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            is_real.clone()
                * op_arith.clone()
                * local.slot_written[s].clone().into()
                * local.slot_is_null[s].clone().into(),
        );
    }
}

/// Transaction index monotonicity: tx_index must be non-decreasing.
fn constrain_tx_index_monotonicity<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let diff: AB::Expr = next.tx_index.clone().into() - local.tx_index.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real * diff.clone() * (diff - AB::Expr::ONE));
}

/// Tau decomposition: `is_access ⟹ tau = reconstruct(tau_rc.limbs)`.
fn constrain_tau_decomposition<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();
    let reconstructed: AB::Expr = local.tau_rc.limbs.limb0.clone().into()
        + local.tau_rc.limbs.limb1.clone().into() * shift_30
        + local.tau_rc.limbs.limb2.clone().into() * shift_60;
    builder.assert_zero(
        is_real * local.is_access.clone().into() * (local.tau.clone().into() - reconstructed),
    );
}

/// Lookup constraint: result binding from access columns.
fn constrain_lookup<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_lookup.clone().into();

    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].clone().into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone()
                    * (local.slots[s][i].clone().into() - local.access_val[i].clone().into()),
            );
        }
        builder.assert_zero(slot_gate * local.slot_is_null[s].clone().into());
    }
}

/// Range-check half-decomposition constraints for access_r, tau_rc, cmp, mul, divmod.
fn constrain_range_check_halves<AB: AirBuilder, const W: usize>(
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
    builder.assert_zero(gate.clone() * r_l1_diff);

    // tau_rc.limbs
    let tau_l0_diff: AB::Expr = local.tau_rc.limbs.limb0.clone().into()
        - (local.tau_rc.l0_halves.lo.clone().into()
            + local.tau_rc.l0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(gate.clone() * tau_l0_diff);

    let tau_l1_diff: AB::Expr = local.tau_rc.limbs.limb1.clone().into()
        - (local.tau_rc.l1_halves.lo.clone().into()
            + local.tau_rc.l1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(gate * tau_l1_diff);

    // access_r and tau_rc limb2 (4-bit boolean decomposition — no gating needed,
    // zero columns satisfy the constraint trivially)
    constrain_limb2_bits(
        builder,
        local.access_r.limbs.limb2.clone().into(),
        &local.access_r.limb2_bits,
    );
    constrain_limb2_bits(
        builder,
        local.tau_rc.limbs.limb2.clone().into(),
        &local.tau_rc.limb2_bits,
    );

    // Cmp inequality diff halves (gated by op_cmp * (1 - cmp_eq_witness))
    let cmp_gate: AB::Expr = is_real_for_cmp
        * local.op_cmp.clone().into()
        * (AB::Expr::ONE - local.cmp_eq_witness.clone().into());

    let cmp_d0_diff: AB::Expr = local.cmp_ineq.diff0.clone().into()
        - (local.cmp_ineq_diff0_halves.lo.clone().into()
            + local.cmp_ineq_diff0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(cmp_gate.clone() * cmp_d0_diff);

    let cmp_d1_diff: AB::Expr = local.cmp_ineq.diff1.clone().into()
        - (local.cmp_ineq_diff1_halves.lo.clone().into()
            + local.cmp_ineq_diff1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(cmp_gate * cmp_d1_diff);

    // Cmp ineq diff2 (4-bit boolean decomposition)
    constrain_limb2_bits(
        builder,
        local.cmp_ineq.diff2.clone().into(),
        &local.cmp_ineq_diff2_bits,
    );

    // Mul carry half-decomposition (gated by op_arith * arith_is_mul)
    let mul_gate: AB::Expr = is_real_for_mul_divmod.clone()
        * local.op_arith.clone().into()
        * local.arith_is_mul.clone().into();
    let mul_c0_diff: AB::Expr = local.mul_c0.clone().into()
        - (local.mul_c0_halves.lo.clone().into()
            + local.mul_c0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(mul_gate * mul_c0_diff);

    // DivMod carry + remainder half-decomposition (gated by op_divmod)
    let divmod_gate: AB::Expr = is_real_for_mul_divmod * local.op_divmod.clone().into();
    let divmod_c0_diff: AB::Expr = local.divmod_c0.clone().into()
        - (local.divmod_c0_halves.lo.clone().into()
            + local.divmod_c0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate.clone() * divmod_c0_diff);

    let divmod_rd0_diff: AB::Expr = local.divmod_rem_ineq.diff0.clone().into()
        - (local.divmod_rem_diff0_halves.lo.clone().into()
            + local.divmod_rem_diff0_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate.clone() * divmod_rd0_diff);

    let divmod_rd1_diff: AB::Expr = local.divmod_rem_ineq.diff1.clone().into()
        - (local.divmod_rem_diff1_halves.lo.clone().into()
            + local.divmod_rem_diff1_halves.hi.clone().into() * expr_from_u32::<AB>(1 << 15));
    builder.assert_zero(divmod_gate * divmod_rd1_diff);

    // DivMod remainder ineq diff2 (4-bit boolean decomposition)
    constrain_limb2_bits(
        builder,
        local.divmod_rem_ineq.diff2.clone().into(),
        &local.divmod_rem_diff2_bits,
    );
}

// ── LogUp bus interactions ──────────────────────────────────────────────────

/// C1 Memory bus send.
fn send_memory<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.is_access.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.access_t.clone().into(),
        local.access_c.clone().into(),
        local.access_r.limbs.limb0.clone().into(),
        local.access_r.limbs.limb1.clone().into(),
        local.access_r.limbs.limb2.clone().into(),
        local.tau_rc.limbs.limb0.clone().into(),
        local.tau_rc.limbs.limb1.clone().into(),
        local.tau_rc.limbs.limb2.clone().into(),
        local.access_is_write.clone().into(),
    ];
    for i in 0..W {
        values.push(local.access_val[i].clone().into());
    }
    values.push(local.access_is_null.clone().into());

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::Memory,
    });
}

/// C8 RangeCheck bus sends.
fn send_range_checks<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into() * local.is_access.clone().into();

    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            kind: InteractionKind::RangeCheck,
        });
    };

    // access_r limbs (limb2 proven by Limb2Bits, no RC send needed)
    send_rc(local.access_r.l0_halves.lo.clone().into());
    send_rc(local.access_r.l0_halves.hi.clone().into());
    send_rc(local.access_r.l1_halves.lo.clone().into());
    send_rc(local.access_r.l1_halves.hi.clone().into());

    // tau limbs (limb2 proven by Limb2Bits, no RC send needed)
    send_rc(local.tau_rc.l0_halves.lo.clone().into());
    send_rc(local.tau_rc.l0_halves.hi.clone().into());
    send_rc(local.tau_rc.l1_halves.lo.clone().into());
    send_rc(local.tau_rc.l1_halves.hi.clone().into());

    // Cmp inequality diff limbs
    let cmp_mult: AB::Expr = local.is_real.clone().into()
        * local.op_cmp.clone().into()
        * (AB::Expr::ONE - local.cmp_eq_witness.clone().into());
    let mut send_cmp_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: cmp_mult.clone(),
            kind: InteractionKind::RangeCheck,
        });
    };
    send_cmp_rc(local.cmp_ineq_diff0_halves.lo.clone().into());
    send_cmp_rc(local.cmp_ineq_diff0_halves.hi.clone().into());
    send_cmp_rc(local.cmp_ineq_diff1_halves.lo.clone().into());
    send_cmp_rc(local.cmp_ineq_diff1_halves.hi.clone().into());
    // diff2 proven by Limb2Bits, no RC send needed

    // Mul carry range checks
    let mul_mult: AB::Expr = local.is_real.clone().into()
        * local.op_arith.clone().into()
        * local.arith_is_mul.clone().into();
    let mut send_mul_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mul_mult.clone(),
            kind: InteractionKind::RangeCheck,
        });
    };
    send_mul_rc(local.mul_c0_halves.lo.clone().into());
    send_mul_rc(local.mul_c0_halves.hi.clone().into());
    send_mul_rc(local.mul_c1_lo.clone().into());
    send_mul_rc(local.mul_c1_hi.clone().into());

    // DivMod range checks
    let divmod_mult: AB::Expr = local.is_real.clone().into() * local.op_divmod.clone().into();
    let mut send_divmod_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: divmod_mult.clone(),
            kind: InteractionKind::RangeCheck,
        });
    };
    send_divmod_rc(local.divmod_c0_halves.lo.clone().into());
    send_divmod_rc(local.divmod_c0_halves.hi.clone().into());
    send_divmod_rc(local.divmod_c1_lo.clone().into());
    send_divmod_rc(local.divmod_c1_hi.clone().into());
    send_divmod_rc(local.divmod_rem_diff0_halves.lo.clone().into());
    send_divmod_rc(local.divmod_rem_diff0_halves.hi.clone().into());
    send_divmod_rc(local.divmod_rem_diff1_halves.lo.clone().into());
    send_divmod_rc(local.divmod_rem_diff1_halves.hi.clone().into());
    // diff2 proven by Limb2Bits, no RC send needed
}

/// C5 PoseidonPermutation bus send for Hash opcode.
fn send_hash_permutation<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.op_hash.clone().into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for i in 0..16 {
        values.push(local.hash_perm_input[i].clone().into());
    }
    for i in 0..8 {
        values.push(local.hash_perm_output[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::PoseidonPermutation,
    });
}

/// C9 StaticTableLookup bus send for Lookup opcode.
fn send_static_table_lookup<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.op_lookup.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.access_t.clone().into(),
        local.access_c.clone().into(),
        local.access_r.limbs.limb0.clone().into(),
        local.access_r.limbs.limb1.clone().into(),
        local.access_r.limbs.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.access_val[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::StaticTableLookup,
    });
}
