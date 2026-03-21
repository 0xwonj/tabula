//! Operand-to-slot linkage constraints for the ExecutionChip.
//!
//! Constraints ensuring operand selectors correctly bind operand witness
//! values to SSA slot values, and that read/write access values flow correctly.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use super::columns::{ExecutionCols, MAX_SLOTS};

/// Operand selector constraints: boolean + exactly-one (gated).
///
/// Each selector array must be boolean per-element, and when the opcode
/// needs that operand, exactly one selector must be 1.
pub(crate) fn constrain_operand_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    // Boolean constraints on all selector elements
    for s in 0..MAX_SLOTS {
        builder.assert_bool(local.src1_sel[s]);
        builder.assert_bool(local.src2_sel[s]);
        builder.assert_bool(local.cond_sel[s]);
    }
    builder.assert_bool(local.src1_is_null);

    // Opcodes that need src1
    let needs_src1: AB::Expr = local.op_arith.into()
        + local.op_divmod.into()
        + local.op_cmp.into()
        + local.op_not.into()
        + local.op_and.into()
        + local.op_or.into()
        + local.op_assert.into()
        + local.op_select.into()
        + local.op_write.into()
        + local.op_hash.into();

    // Opcodes that need src2
    let needs_src2: AB::Expr = local.op_arith.into()
        + local.op_divmod.into()
        + local.op_cmp.into()
        + local.op_and.into()
        + local.op_or.into()
        + local.op_select.into()
        + local.op_hash.into();

    // Opcodes that need cond
    let needs_cond: AB::Expr = local.op_select.into();

    // Sum of selectors
    let src1_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.src1_sel[s].into()).sum();
    let src2_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.src2_sel[s].into()).sum();
    let cond_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.cond_sel[s].into()).sum();

    // Exactly-one when needed
    builder.assert_zero(is_real.clone() * needs_src1 * (src1_sum - AB::Expr::ONE));
    builder.assert_zero(is_real.clone() * needs_src2 * (src2_sum - AB::Expr::ONE));
    builder.assert_zero(is_real * needs_cond * (cond_sum - AB::Expr::ONE));
}

/// Operand value linkage: selector gates operand-to-slot equality.
///
/// For each slot s and limb i:
///   `src1_sel[s] * (src1_val[i] - slots[s][i]) = 0`
///   `src2_sel[s] * (src2_val[i] - slots[s][i]) = 0`
///   `src1_sel[s] * (src1_is_null - slot_is_null[s]) = 0`
///   `cond_sel[s] * (cond_val - slots[s][0]) = 0`
pub(crate) fn constrain_operand_value_linkage<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    for s in 0..MAX_SLOTS {
        let sel1: AB::Expr = local.src1_sel[s].into();
        let sel2: AB::Expr = local.src2_sel[s].into();
        let selc: AB::Expr = local.cond_sel[s].into();

        for i in 0..W {
            // src1 linkage
            builder
                .assert_zero(sel1.clone() * (local.src1_val[i].into() - local.slots[s][i].into()));
            // src2 linkage
            builder
                .assert_zero(sel2.clone() * (local.src2_val[i].into() - local.slots[s][i].into()));
        }

        // src1 null flag linkage
        builder.assert_zero(sel1 * (local.src1_is_null.into() - local.slot_is_null[s].into()));

        // cond linkage (single boolean from limb 0)
        builder.assert_zero(selc * (local.cond_val.into() - local.slots[s][0].into()));
    }
}

/// Write operand constraint: access_val must equal src1_val for writes.
///
/// `is_real * op_write * (access_val[i] - src1_val[i]) = 0`
/// `is_real * op_write * (access_is_null - src1_is_null) = 0`
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_write_operand<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.op_write.into();

    for i in 0..W {
        builder.assert_zero(gate.clone() * (local.access_val[i].into() - local.src1_val[i].into()));
    }
    builder.assert_zero(gate * (local.access_is_null.into() - local.src1_is_null.into()));
}

/// Read destination constraint: read value flows to the written slot.
///
/// For each written slot s:
///   `is_real * op_read * slot_written[s] * (slots[s][i] - access_val[i]) = 0`
///   `is_real * op_read * slot_written[s] * (slot_is_null[s] - access_is_null) = 0`
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_read_destination<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    for s in 0..MAX_SLOTS {
        let gate: AB::Expr = is_real.clone() * local.op_read.into() * local.slot_written[s].into();

        for i in 0..W {
            builder.assert_zero(
                gate.clone() * (local.slots[s][i].into() - local.access_val[i].into()),
            );
        }
        builder.assert_zero(gate * (local.slot_is_null[s].into() - local.access_is_null.into()));
    }
}
