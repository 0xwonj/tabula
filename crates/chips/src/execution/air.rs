//! ExecutionChip — AIR constraints for the instruction trace.
//!
//! One row per instruction. Constraints enforce:
//! 1. Boolean fields: all opcode selectors, is_access, access_is_write, slot_written, etc.
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Opcode exactly-one: sum of 13 opcode selectors = 1 when is_real
//! 4. `is_access` derived: is_access = op_read + op_write
//! 5. Clock recurrence: clk increments by is_access; first row clk=0
//! 6. Access log: access_is_write = op_write when is_access
//! 7. SSA slot carry: non-written slots carry forward to next row
//! 8. Arith sub-selectors: exactly one of {add, sub, mul} when op_arith
//! 9. Per-opcode semantics (delegated to `ops/`)
//! 10. Transaction index monotonicity
//! 11. Operand-to-slot linkage (delegated to `linkage`)
//! 12. Empty column flag: is_empty_col → op_read ∧ access_is_null
//! 13. Relation tuple binding and digest relay.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

/// The ExecutionChip AIR, generic over value width.
#[derive(Clone, Copy, Debug)]
pub struct ExecutionChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for ExecutionChip<W> {
    fn width(&self) -> usize {
        execution_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for ExecutionChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &ExecutionCols<AB::Var, W> = borrow_cols(local_row);
        let next: &ExecutionCols<AB::Var, W> = borrow_cols(next_row);

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();

        // ── Structural constraints ──
        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real, next.is_real);
        constrain_opcode_one_hot(builder, local, is_real.clone());
        constrain_is_access(builder, local, is_real.clone());
        constrain_clock(builder, local, next, both_real.clone());
        constrain_access_log(builder, local, is_real.clone());
        constrain_empty_col(builder, local, is_real.clone());
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
        super::ops::control::constrain_load_immediate(builder, local, is_real.clone());
        super::ops::logic::constrain_not(builder, local, is_real.clone());
        super::ops::logic::constrain_and(builder, local, is_real.clone());
        super::ops::logic::constrain_or(builder, local, is_real.clone());
        super::ops::hash::constrain_hash(builder, local, is_real.clone());
        super::ops::capability_call::constrain_capability_call(builder, local, is_real.clone());
        super::ops::property_read::constrain_property_read(builder, local, is_real.clone());
        super::ops::relation::constrain_relation_table(builder, local, is_real.clone());
        constrain_lookup(builder, local, is_real.clone());
        constrain_tx_index_monotonicity(builder, local, next, both_real);

        // ── Operand-to-slot linkage ──
        super::linkage::constrain_operand_selectors(builder, local, is_real.clone());
        super::linkage::constrain_operand_value_linkage(builder, local);
        super::linkage::constrain_write_operand(builder, local, is_real.clone());
        super::range_checks::constrain_range_check_halves(builder, local, is_real.clone());
        super::linkage::constrain_read_destination(builder, local, is_real);

        // ── LogUp buses ──
        super::buses::send_read_access(builder, local);
        super::buses::send_write_access(builder, local);
        super::buses::send_empty_col_read(builder, local);
        super::buses::send_range_checks(builder, local);
        super::buses::send_hash_relay(builder, local);
        super::buses::send_capability_call(builder, local);
        super::buses::send_public_context_transcript_item(builder, local);
        super::buses::send_tx_batch_transcript_item(builder, local);
        super::buses::send_event_transcript_item(builder, local);
        super::buses::send_property_read(builder, local);
        super::buses::send_static_table_lookup(builder, local);
        super::buses::send_relation_tuples(builder, local);
        super::buses::receive_relation_digests(builder, local);
        super::buses::send_relation_table(builder, local);
    }
}

// ── Structural constraint helpers ───────────────────────────────────────────

/// 1. Boolean constraints on all selector and flag columns.
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    // Opcode selectors
    builder.assert_bool(local.op_read);
    builder.assert_bool(local.op_write);
    builder.assert_bool(local.op_arith);
    builder.assert_bool(local.op_divmod);
    builder.assert_bool(local.op_cmp);
    builder.assert_bool(local.op_not);
    builder.assert_bool(local.op_and);
    builder.assert_bool(local.op_or);
    builder.assert_bool(local.op_assert);
    builder.assert_bool(local.op_select);
    builder.assert_bool(local.op_hash);
    builder.assert_bool(local.op_lookup);
    builder.assert_bool(local.op_capability_call);
    builder.assert_bool(local.op_property_read);
    builder.assert_bool(local.op_relation_table);
    builder.assert_bool(local.op_tx_begin);
    builder.assert_bool(local.op_load_param);
    builder.assert_bool(local.op_load_context);
    builder.assert_bool(local.op_emit_event_header);
    builder.assert_bool(local.op_emit_event_arg);

    // Arith sub-selectors
    builder.assert_bool(local.arith_is_sub);
    builder.assert_bool(local.arith_is_mul);

    // Flags
    builder.assert_bool(local.is_access);
    builder.assert_bool(local.is_empty_col);
    builder.assert_bool(local.access_is_write);
    builder.assert_bool(local.access_is_null);
    builder.assert_bool(local.cond_val);
    builder.assert_bool(local.carry0);
    builder.assert_bool(local.carry1);
    builder.assert_bool(local.relation_is_eval);

    // Cmp sub-selectors and witnesses
    builder.assert_bool(local.cmp.is_eq);
    builder.assert_bool(local.cmp.is_ne);
    builder.assert_bool(local.cmp.is_lt);
    builder.assert_bool(local.cmp.is_lte);
    builder.assert_bool(local.cmp.is_gt);
    builder.assert_bool(local.cmp.is_gte);
    builder.assert_bool(local.cmp.lt_witness);
    builder.assert_bool(local.cmp.eq_witness);

    // Per-slot flags
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.slot_is_null[s]);
        builder.assert_bool(local.slot_written[s]);
        builder.assert_bool(local.relation_input_used[s]);
        builder.assert_bool(local.relation_output_used[s]);
        for idx in 0..MAX_SLOTS {
            builder.assert_bool(local.relation_input_sel[s][idx]);
            builder.assert_bool(local.relation_output_sel[s][idx]);
        }
    }
}

/// 3. Opcode exactly-one: sum of opcode selectors = 1 when is_real.
fn constrain_opcode_one_hot<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let opcode_sum: AB::Expr = local.op_read.into()
        + local.op_write.into()
        + local.op_arith.into()
        + local.op_divmod.into()
        + local.op_cmp.into()
        + local.op_not.into()
        + local.op_and.into()
        + local.op_or.into()
        + local.op_assert.into()
        + local.op_select.into()
        + local.op_hash.into()
        + local.op_lookup.into()
        + local.op_capability_call.into()
        + local.op_property_read.into()
        + local.op_relation_table.into()
        + local.op_tx_begin.into()
        + local.op_load_param.into()
        + local.op_load_context.into()
        + local.op_emit_event_header.into()
        + local.op_emit_event_arg.into();

    builder.assert_zero(is_real * (opcode_sum - AB::Expr::ONE));
}

/// 4. `is_access` derived: is_access = op_read + op_write.
fn constrain_is_access<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let derived: AB::Expr = local.op_read.into() + local.op_write.into();
    builder.assert_zero(is_real * (local.is_access.into() - derived));
}

/// 5. Clock recurrence: next.clk = local.clk + local.is_access.
fn constrain_clock<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let clk_diff: AB::Expr = next.clk.into() - local.clk.into() - local.is_access.into();
    builder.when_transition().assert_zero(both_real * clk_diff);
}

/// 6. Empty column flag semantics.
///
/// `is_empty_col = 1` implies `op_read = 1` and `access_is_null = 1`.
#[allow(clippy::needless_pass_by_value)]
fn constrain_empty_col<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.is_empty_col.into();
    // is_empty_col → op_read
    builder.assert_zero(gate.clone() * (AB::Expr::ONE - local.op_read.into()));
    // is_empty_col → access_is_null
    builder.assert_zero(gate * (AB::Expr::ONE - local.access_is_null.into()));
}

/// 7. Access log: access_is_write = op_write when is_access.
fn constrain_access_log<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.is_access.into();
    builder.assert_zero(gate * (local.access_is_write.into() - local.op_write.into()));
}

/// 8. SSA slot carry: slots not written by the NEXT instruction carry forward.
#[allow(clippy::needless_pass_by_value)]
fn constrain_slot_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    next: &ExecutionCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    for s in 0..MAX_SLOTS {
        let not_written_next: AB::Expr = AB::Expr::ONE - next.slot_written[s].into();
        let gate: AB::Expr = both_real.clone() * not_written_next;

        for i in 0..W {
            let diff: AB::Expr = next.slots[s][i].into() - local.slots[s][i].into();
            builder.when_transition().assert_zero(gate.clone() * diff);
        }

        let null_diff: AB::Expr = next.slot_is_null[s].into() - local.slot_is_null[s].into();
        builder.when_transition().assert_zero(gate * null_diff);
    }
}

/// 9. Arith sub-selectors: when op_arith, exactly one of {add, sub, mul}.
fn constrain_arith_sub_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_arith: AB::Expr = local.op_arith.into();

    builder.assert_zero(
        is_real.clone() * op_arith.clone() * local.arith_is_sub.into() * local.arith_is_mul.into(),
    );

    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - op_arith.clone()) * local.arith_is_sub.into(),
    );
    builder.assert_zero(is_real * (AB::Expr::ONE - op_arith) * local.arith_is_mul.into());
}

/// 10a. First-row initialization: clk starts at zero, non-written slots start zeroed.
fn constrain_first_row_init<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    builder
        .when_first_row()
        .when(local.is_real)
        .assert_zero(local.clk);

    for s in 0..MAX_SLOTS {
        let not_written: AB::Expr = AB::Expr::ONE - local.slot_written[s].into();
        for i in 0..W {
            builder
                .when_first_row()
                .when(local.is_real)
                .assert_zero(not_written.clone() * local.slots[s][i].into());
        }
        builder
            .when_first_row()
            .when(local.is_real)
            .assert_zero(not_written * local.slot_is_null[s].into());
    }
}

/// Slot written count constraint: total `slot_written` flags must match the opcode.
fn constrain_slot_written_count<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let written_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.slot_written[s].into()).sum();
    let relation_output_count: AB::Expr = (0..MAX_SLOTS)
        .map(|idx| local.relation_output_used[idx].into())
        .sum();

    let default_expected: AB::Expr = AB::Expr::ONE
        - local.op_write.into()
        - local.op_assert.into()
        - local.op_capability_call.into()
        - local.op_tx_begin.into()
        - local.op_emit_event_header.into()
        - local.op_emit_event_arg.into()
        + local.op_divmod.into()
        + local.op_property_read.into() * (AB::Expr::ONE + AB::Expr::ONE);
    let expected = default_expected
        + local.op_capability_call.into() * local.capability_output_count.into()
        + local.op_relation_table.into() * (relation_output_count - AB::Expr::ONE);

    builder.assert_zero(is_real * (written_sum - expected));
}

/// Arithmetic result null constraint: written slots must not be null.
#[allow(clippy::needless_pass_by_value)]
fn constrain_arith_result_not_null<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let op_arith: AB::Expr = local.op_arith.into();
    for s in 0..MAX_SLOTS {
        builder.assert_zero(
            is_real.clone()
                * op_arith.clone()
                * local.slot_written[s].into()
                * local.slot_is_null[s].into(),
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
    let diff: AB::Expr = next.tx_index.into() - local.tx_index.into();
    builder
        .when_transition()
        .assert_zero(both_real * diff.clone() * (diff - AB::Expr::ONE));
}

/// Lookup constraint: result binding from access columns.
fn constrain_lookup<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.op_lookup.into();

    for s in 0..MAX_SLOTS {
        let slot_gate: AB::Expr = gate.clone() * local.slot_written[s].into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone() * (local.slots[s][i].into() - local.access_val[i].into()),
            );
        }
        builder.assert_zero(slot_gate * local.slot_is_null[s].into());
    }
}
