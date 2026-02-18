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
//! 10. Per-opcode semantics (M8-4a: Add, Sub, Assert, Select; M9-A3: Not, And, Or)
//! 11. Transaction index monotonicity (M9-A3)
//! 12. Operand-to-slot linkage (M9-A1: src1/src2/cond selectors, value/null matching)
//!
//! NOT constrained yet (deferred to M10):
//! - Hash/Lookup bus interactions
//! - Range checks on arithmetic carry/limbs

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::columns::borrow_cols;
use crate::air::gadgets::constrain_is_real_prefix;
use crate::air::gadgets::integer::{SHIFT_30_U32, expr_from_u32};
use crate::air::interaction::{AirInteraction, InteractionKind};

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

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

        constrain_booleans(builder, local);
        constrain_is_real(builder, local, next);
        constrain_opcode_one_hot(builder, local, is_real.clone());
        constrain_is_access(builder, local, is_real.clone());
        constrain_clock(builder, local, next, both_real.clone());
        constrain_timestamp(builder, local, is_real.clone());
        constrain_access_log(builder, local, is_real.clone());
        constrain_arith_sub_selectors(builder, local, is_real.clone());
        constrain_slot_carry(builder, local, next, both_real.clone());
        constrain_first_row_init(builder, local);

        // Per-opcode semantics (M8-4a + M9-A3)
        constrain_arith_add(builder, local, is_real.clone());
        constrain_arith_sub(builder, local, is_real.clone());
        constrain_assert(builder, local, is_real.clone());
        constrain_select(builder, local, is_real.clone());
        constrain_not(builder, local, is_real.clone());
        constrain_and(builder, local, is_real.clone());
        constrain_or(builder, local, is_real.clone());
        constrain_arith_result_not_null(builder, local, is_real.clone());
        constrain_tx_index_monotonicity(builder, local, next, both_real);
        constrain_tau_decomposition(builder, local, is_real.clone());

        // Operand-to-slot linkage (M9 A1)
        constrain_operand_selectors(builder, local, is_real.clone());
        constrain_operand_value_linkage(builder, local);
        constrain_write_operand(builder, local, is_real.clone());
        constrain_read_destination(builder, local, is_real);

        // ── LogUp buses ──
        send_memory(builder, local);
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

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

    // Per-slot flags
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.slot_is_null[s].clone());
        builder.assert_bool(local.slot_written[s].clone());
    }
}

/// 2. `is_real` prefix: monotonic 1→0.
fn constrain_is_real<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
) {
    constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
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

    // is_real ⟹ opcode_sum = 1
    builder.assert_zero(is_real * (opcode_sum - AB::Expr::ONE));
}

/// 4. `is_access` derived: is_access = op_read + op_write.
fn constrain_is_access<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let derived: AB::Expr = local.op_read.clone().into() + local.op_write.clone().into();

    // is_real ⟹ is_access = op_read + op_write
    builder.assert_zero(is_real * (local.is_access.clone().into() - derived));
}

/// 5. Clock recurrence.
///
/// - First row: clk = 0 (handled by `constrain_first_row_init`)
/// - Transition: next.clk = local.clk + local.is_access
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

    // is_real * is_access * (tau - clk - 1) = 0
    builder.assert_zero(is_real * local.is_access.clone().into() * tau_diff);
}

/// 7. Access log: access_is_write = op_write when is_access.
fn constrain_access_log<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    // is_real * is_access ⟹ access_is_write = op_write
    let gate: AB::Expr = is_real * local.is_access.clone().into();
    builder
        .assert_zero(gate * (local.access_is_write.clone().into() - local.op_write.clone().into()));
}

/// 8. SSA slot carry: slots not written by the NEXT instruction carry forward.
///
/// For each slot s and limb i:
/// `both_real * (1 - next.slot_written[s]) * (next.slots[s][i] - local.slots[s][i]) = 0`
///
/// If the next instruction writes to slot s, its value is set by the opcode.
/// If not, it must equal the current row's value (carry).
fn constrain_slot_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    for s in 0..MAX_SLOTS {
        let not_written_next: AB::Expr = AB::Expr::ONE - next.slot_written[s].clone().into();
        let gate: AB::Expr = both_real.clone() * not_written_next;

        // Value carry
        for i in 0..W {
            let diff: AB::Expr = next.slots[s][i].clone().into() - local.slots[s][i].clone().into();
            builder.when_transition().assert_zero(gate.clone() * diff);
        }

        // Null flag carry
        let null_diff: AB::Expr =
            next.slot_is_null[s].clone().into() - local.slot_is_null[s].clone().into();
        builder.when_transition().assert_zero(gate * null_diff);
    }
}

/// 9. Arith sub-selectors: when op_arith, exactly one of {add, sub, mul}.
///
/// Constraint: op_arith ⟹ arith_is_sub + arith_is_mul ∈ {0, 1}
/// (add is implicit: op_arith * (1 - arith_is_sub) * (1 - arith_is_mul))
///
/// Also: arith sub-selectors must be 0 when not op_arith.
fn constrain_arith_sub_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_arith: AB::Expr = local.op_arith.clone().into();

    // arith_is_sub + arith_is_mul <= 1 (both boolean, so product = 0 enforces mutual exclusion)
    builder.assert_zero(
        is_real.clone()
            * op_arith.clone()
            * local.arith_is_sub.clone().into()
            * local.arith_is_mul.clone().into(),
    );

    // arith_is_sub = 0 when not op_arith
    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - op_arith.clone()) * local.arith_is_sub.clone().into(),
    );
    // arith_is_mul = 0 when not op_arith
    builder.assert_zero(is_real * (AB::Expr::ONE - op_arith) * local.arith_is_mul.clone().into());
}

/// 10a. First-row initialization: clk starts at zero, non-written slots start zeroed.
fn constrain_first_row_init<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    // First row of trace: clk = 0
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.clk.clone());

    // First row: non-written slots must be zero (initial SSA state).
    // Slot carry only applies to transitions (row i → i+1), so the first
    // row's non-written slots need explicit zeroing.
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

// ── Per-opcode semantics (M8-4a) ──────────────────────────────────────────

/// Arith(Add) constraint: integer add via limb carry chain.
///
/// For each written slot s:
///   slots[s][0] + carry0 * 2^30 = src1_val[0] + src2_val[0]
///   slots[s][1] + carry1 * 2^30 = src1_val[1] + src2_val[1] + carry0
///   slots[s][2]                  = src1_val[2] + src2_val[2] + carry1
fn constrain_arith_add<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    if W < 3 {
        return; // Only applicable for Standard width
    }

    let op_add: AB::Expr = local.op_arith.clone().into()
        * (AB::Expr::ONE - local.arith_is_sub.clone().into())
        * (AB::Expr::ONE - local.arith_is_mul.clone().into());

    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    // For each slot that could be the destination
    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_add.clone() * local.slot_written[s].clone().into();

        // Limb 0: slots[s][0] + carry0 * 2^30 = src1[0] + src2[0]
        let lhs0: AB::Expr =
            local.slots[s][0].clone().into() + local.carry0.clone().into() * shift_30.clone();
        let rhs0: AB::Expr = local.src1_val[0].clone().into() + local.src2_val[0].clone().into();
        builder.assert_zero(gate.clone() * (lhs0 - rhs0));

        // Limb 1: slots[s][1] + carry1 * 2^30 = src1[1] + src2[1] + carry0
        let lhs1: AB::Expr =
            local.slots[s][1].clone().into() + local.carry1.clone().into() * shift_30.clone();
        let rhs1: AB::Expr = local.src1_val[1].clone().into()
            + local.src2_val[1].clone().into()
            + local.carry0.clone().into();
        builder.assert_zero(gate.clone() * (lhs1 - rhs1));

        // Limb 2: slots[s][2] = src1[2] + src2[2] + carry1
        let lhs2: AB::Expr = local.slots[s][2].clone().into();
        let rhs2: AB::Expr = local.src1_val[2].clone().into()
            + local.src2_val[2].clone().into()
            + local.carry1.clone().into();
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
fn constrain_arith_sub<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    if W < 3 {
        return;
    }

    let op_sub: AB::Expr = local.op_arith.clone().into() * local.arith_is_sub.clone().into();
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_sub.clone() * local.slot_written[s].clone().into();

        // Limb 0: slots[s][0] = src1[0] - src2[0] + carry0 * 2^30
        let expected0: AB::Expr = local.src1_val[0].clone().into()
            - local.src2_val[0].clone().into()
            + local.carry0.clone().into() * shift_30.clone();
        builder.assert_zero(gate.clone() * (local.slots[s][0].clone().into() - expected0));

        // Limb 1: slots[s][1] = src1[1] - src2[1] - carry0 + carry1 * 2^30
        let expected1: AB::Expr = local.src1_val[1].clone().into()
            - local.src2_val[1].clone().into()
            - local.carry0.clone().into()
            + local.carry1.clone().into() * shift_30.clone();
        builder.assert_zero(gate.clone() * (local.slots[s][1].clone().into() - expected1));

        // Limb 2: slots[s][2] = src1[2] - src2[2] - carry1
        let expected2: AB::Expr = local.src1_val[2].clone().into()
            - local.src2_val[2].clone().into()
            - local.carry1.clone().into();
        builder.assert_zero(gate * (local.slots[s][2].clone().into() - expected2));
    }
}

/// Assert constraint: condition value must be 1.
///
/// op_assert ⟹ src1_val[0] = 1
fn constrain_assert<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_assert.clone().into();
    builder.assert_zero(gate * (local.src1_val[0].clone().into() - AB::Expr::ONE));
}

/// Select constraint: conditional value selection.
///
/// For each written slot s:
///   slots[s][i] = cond * src1_val[i] + (1 - cond) * src2_val[i]
///
/// Simplified: slots[s][i] = src2_val[i] + cond * (src1_val[i] - src2_val[i])
fn constrain_select<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_select: AB::Expr = local.op_select.clone().into();
    let cond: AB::Expr = local.cond_val.clone().into();

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_select.clone() * local.slot_written[s].clone().into();

        for i in 0..W {
            let expected: AB::Expr = local.src2_val[i].clone().into()
                + cond.clone()
                    * (local.src1_val[i].clone().into() - local.src2_val[i].clone().into());
            builder.assert_zero(gate.clone() * (local.slots[s][i].clone().into() - expected));
        }
    }
}

/// Not constraint: boolean negation.
///
/// For each written slot s:
///   slots[s][0] = 1 − src1_val[0]   (boolean negation)
///   slots[s][i] = 0                   for i ∈ {1, 2} (higher limbs zero)
fn constrain_not<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_not: AB::Expr = local.op_not.clone().into();

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
///   slots[s][0] = src1_val[0] · src2_val[0]
///   slots[s][i] = 0   for i ∈ {1, 2}
fn constrain_and<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_and: AB::Expr = local.op_and.clone().into();

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * op_and.clone() * local.slot_written[s].clone().into();

        // Limb 0: dst = src1 · src2
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
///   slots[s][0] = src1_val[0] + src2_val[0] − src1_val[0] · src2_val[0]
///   slots[s][i] = 0   for i ∈ {1, 2}
fn constrain_or<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_or: AB::Expr = local.op_or.clone().into();

    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * op_or.clone() * local.slot_written[s].clone().into();

        // Limb 0: dst = src1 + src2 - src1 · src2
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

/// Arithmetic result null constraint: written slots must not be null.
///
/// All arithmetic operations (Add, Sub, Mul) produce non-null results.
/// is_real * op_arith * slot_written[s] * slot_is_null[s] = 0
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
///
/// Constraint: `both_real · (next.tx_index − local.tx_index) · (next.tx_index − local.tx_index − 1) = 0`
///
/// This ensures `next.tx_index − local.tx_index ∈ {0, 1}` for consecutive real rows.
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

/// Tau decomposition: `is_access ⟹ tau = reconstruct(tau_limbs)`.
///
/// Ensures the single-FE `tau` matches its 3-limb decomposition for Memory bus.
fn constrain_tau_decomposition<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();
    let reconstructed: AB::Expr = local.tau_limbs.limb0.clone().into()
        + local.tau_limbs.limb1.clone().into() * shift_30
        + local.tau_limbs.limb2.clone().into() * shift_60;
    builder.assert_zero(
        is_real * local.is_access.clone().into() * (local.tau.clone().into() - reconstructed),
    );
}

// ── Operand-to-slot linkage (M9 A1) ────────────────────────────────────────

/// 12a. Operand selector constraints: boolean + exactly-one (gated).
///
/// Each selector array must be boolean per-element, and when the opcode
/// needs that operand, exactly one selector must be 1.
fn constrain_operand_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    // Boolean constraints on all selector elements
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.src1_sel[s].clone());
        builder.assert_bool(local.src2_sel[s].clone());
        builder.assert_bool(local.cond_sel[s].clone());
    }
    builder.assert_bool(local.src1_is_null.clone());

    // Opcodes that need src1
    let needs_src1: AB::Expr = local.op_arith.clone().into()
        + local.op_cmp.clone().into()
        + local.op_not.clone().into()
        + local.op_and.clone().into()
        + local.op_or.clone().into()
        + local.op_assert.clone().into()
        + local.op_select.clone().into()
        + local.op_write.clone().into();

    // Opcodes that need src2
    let needs_src2: AB::Expr = local.op_arith.clone().into()
        + local.op_cmp.clone().into()
        + local.op_and.clone().into()
        + local.op_or.clone().into()
        + local.op_select.clone().into();

    // Opcodes that need cond
    let needs_cond: AB::Expr = local.op_select.clone().into();

    // Sum of selectors
    let src1_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.src1_sel[s].clone().into())
        .sum();
    let src2_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.src2_sel[s].clone().into())
        .sum();
    let cond_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|s| local.cond_sel[s].clone().into())
        .sum();

    // Exactly-one when needed: is_real * needs_src1 * (sum - 1) = 0
    builder.assert_zero(is_real.clone() * needs_src1 * (src1_sum - AB::Expr::ONE));
    builder.assert_zero(is_real.clone() * needs_src2 * (src2_sum - AB::Expr::ONE));
    builder.assert_zero(is_real * needs_cond * (cond_sum - AB::Expr::ONE));
}

/// 12b. Operand value linkage: selector gates operand-to-slot equality.
///
/// For each slot s and limb i:
///   `src1_sel[s] * (src1_val[i] - slots[s][i]) = 0`
///   `src2_sel[s] * (src2_val[i] - slots[s][i]) = 0`
///   `src1_sel[s] * (src1_is_null - slot_is_null[s]) = 0`
///   `cond_sel[s] * (cond_val - slots[s][0]) = 0`
fn constrain_operand_value_linkage<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    for s in 0..MAX_SLOTS {
        let sel1: AB::Expr = local.src1_sel[s].clone().into();
        let sel2: AB::Expr = local.src2_sel[s].clone().into();
        let selc: AB::Expr = local.cond_sel[s].clone().into();

        for i in 0..W {
            // src1 linkage
            builder.assert_zero(
                sel1.clone()
                    * (local.src1_val[i].clone().into() - local.slots[s][i].clone().into()),
            );
            // src2 linkage
            builder.assert_zero(
                sel2.clone()
                    * (local.src2_val[i].clone().into() - local.slots[s][i].clone().into()),
            );
        }

        // src1 null flag linkage
        builder.assert_zero(
            sel1 * (local.src1_is_null.clone().into() - local.slot_is_null[s].clone().into()),
        );

        // cond linkage (single boolean from limb 0)
        builder
            .assert_zero(selc * (local.cond_val.clone().into() - local.slots[s][0].clone().into()));
    }
}

/// 12c. Write operand constraint: access_val must equal src1_val for writes.
///
/// `is_real * op_write * (access_val[i] - src1_val[i]) = 0`
/// `is_real * op_write * (access_is_null - src1_is_null) = 0`
fn constrain_write_operand<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.op_write.clone().into();

    for i in 0..W {
        builder.assert_zero(
            gate.clone() * (local.access_val[i].clone().into() - local.src1_val[i].clone().into()),
        );
    }
    builder.assert_zero(
        gate * (local.access_is_null.clone().into() - local.src1_is_null.clone().into()),
    );
}

/// 12d. Read destination constraint: read value flows to the written slot.
///
/// For each written slot s:
///   `is_real * op_read * slot_written[s] * (slots[s][i] - access_val[i]) = 0`
///   `is_real * op_read * slot_written[s] * (slot_is_null[s] - access_is_null) = 0`
fn constrain_read_destination<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    for s in 0..MAX_SLOTS {
        let gate: AB::Expr =
            is_real.clone() * local.op_read.clone().into() * local.slot_written[s].clone().into();

        for i in 0..W {
            builder.assert_zero(
                gate.clone()
                    * (local.slots[s][i].clone().into() - local.access_val[i].clone().into()),
            );
        }
        builder.assert_zero(
            gate * (local.slot_is_null[s].clone().into() - local.access_is_null.clone().into()),
        );
    }
}

// ── LogUp bus interactions ──────────────────────────────────────────────────

/// C1 Memory bus send: execution access rows → GlobalSortedMem.
///
/// Tuple: `(access_t, access_c, access_r[3], tau_limbs[3], access_is_write, access_val[W], access_is_null)`.
/// Multiplicity: `is_real · is_access`.
fn send_memory<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.is_access.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.access_t.clone().into(),
        local.access_c.clone().into(),
        local.access_r.limb0.clone().into(),
        local.access_r.limb1.clone().into(),
        local.access_r.limb2.clone().into(),
        local.tau_limbs.limb0.clone().into(),
        local.tau_limbs.limb1.clone().into(),
        local.tau_limbs.limb2.clone().into(),
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
