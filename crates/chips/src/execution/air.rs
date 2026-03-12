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

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;

use super::columns::{ExecutionCols, MAX_SLOTS, execution_width};

/// Domain tag for the instruction-level Hash opcode.
///
/// Distinct from protocol-level domain tags (0x00=SSMC, 0x01=SMT, 0x10=leaf,
/// 0x11=tables, 0x12=cols) to prevent cross-protocol hash collisions.
pub const HASH_INSTRUCTION_DOMAIN_TAG: u32 = 0x20;

/// Number of input values for the Hash instruction (always 2: src1 and src2).
pub const HASH_INSTRUCTION_INPUT_COUNT: u32 = 2;

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
        super::ops::logic::constrain_not(builder, local, is_real.clone());
        super::ops::logic::constrain_and(builder, local, is_real.clone());
        super::ops::logic::constrain_or(builder, local, is_real.clone());
        super::ops::hash::constrain_hash(builder, local, is_real.clone());
        super::ops::precompile::constrain_precompile(builder, local, is_real.clone());
        super::ops::property_read::constrain_property_read(builder, local, is_real.clone());
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
        super::buses::send_hash_permutation(builder, local);
        super::buses::send_precompile(builder, local);
        super::buses::send_property_read(builder, local);
        super::buses::send_static_table_lookup(builder, local);
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
    builder.assert_bool(local.op_precompile.clone());
    builder.assert_bool(local.op_property_read.clone());

    // Arith sub-selectors
    builder.assert_bool(local.arith_is_sub.clone());
    builder.assert_bool(local.arith_is_mul.clone());

    // Flags
    builder.assert_bool(local.is_access.clone());
    builder.assert_bool(local.is_empty_col.clone());
    builder.assert_bool(local.access_is_write.clone());
    builder.assert_bool(local.access_is_null.clone());
    builder.assert_bool(local.cond_val.clone());
    builder.assert_bool(local.carry0.clone());
    builder.assert_bool(local.carry1.clone());

    // Cmp sub-selectors and witnesses
    builder.assert_bool(local.cmp.is_eq.clone());
    builder.assert_bool(local.cmp.is_ne.clone());
    builder.assert_bool(local.cmp.is_lt.clone());
    builder.assert_bool(local.cmp.is_lte.clone());
    builder.assert_bool(local.cmp.is_gt.clone());
    builder.assert_bool(local.cmp.is_gte.clone());
    builder.assert_bool(local.cmp.lt_witness.clone());
    builder.assert_bool(local.cmp.eq_witness.clone());

    // Per-slot flags
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.slot_is_null[s].clone());
        builder.assert_bool(local.slot_written[s].clone());
    }
}

/// 3. Opcode exactly-one: sum of 14 selectors = 1 when is_real.
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
        + local.op_lookup.clone().into()
        + local.op_precompile.clone().into()
        + local.op_property_read.clone().into();

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

/// 6. Empty column flag semantics.
///
/// `is_empty_col = 1` implies `op_read = 1` and `access_is_null = 1`.
#[allow(clippy::needless_pass_by_value)]
fn constrain_empty_col<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.is_empty_col.clone().into();
    // is_empty_col → op_read
    builder.assert_zero(gate.clone() * (AB::Expr::ONE - local.op_read.clone().into()));
    // is_empty_col → access_is_null
    builder.assert_zero(gate * (AB::Expr::ONE - local.access_is_null.clone().into()));
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
#[allow(clippy::needless_pass_by_value)]
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
            + local.op_divmod.clone().into()
            + local.op_property_read.clone().into() * AB::Expr::TWO;

    builder.assert_zero(is_real * (written_sum - expected));
}

/// Arithmetic result null constraint: written slots must not be null.
#[allow(clippy::needless_pass_by_value)]
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
