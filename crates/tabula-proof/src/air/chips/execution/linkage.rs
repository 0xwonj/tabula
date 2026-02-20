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
        builder.assert_bool(local.src1_sel[s].clone());
        builder.assert_bool(local.src2_sel[s].clone());
        builder.assert_bool(local.cond_sel[s].clone());
    }
    builder.assert_bool(local.src1_is_null.clone());

    // Opcodes that need src1
    let needs_src1: AB::Expr = local.op_arith.clone().into()
        + local.op_divmod.clone().into()
        + local.op_cmp.clone().into()
        + local.op_not.clone().into()
        + local.op_and.clone().into()
        + local.op_or.clone().into()
        + local.op_assert.clone().into()
        + local.op_select.clone().into()
        + local.op_write.clone().into()
        + local.op_hash.clone().into();

    // Opcodes that need src2
    let needs_src2: AB::Expr = local.op_arith.clone().into()
        + local.op_divmod.clone().into()
        + local.op_cmp.clone().into()
        + local.op_and.clone().into()
        + local.op_or.clone().into()
        + local.op_select.clone().into()
        + local.op_hash.clone().into();

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

/// Write operand constraint: access_val must equal src1_val for writes.
///
/// `is_real * op_write * (access_val[i] - src1_val[i]) = 0`
/// `is_real * op_write * (access_is_null - src1_is_null) = 0`
pub(crate) fn constrain_write_operand<AB: AirBuilder, const W: usize>(
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

/// Read destination constraint: read value flows to the written slot.
///
/// For each written slot s:
///   `is_real * op_read * slot_written[s] * (slots[s][i] - access_val[i]) = 0`
///   `is_real * op_read * slot_written[s] * (slot_is_null[s] - access_is_null) = 0`
pub(crate) fn constrain_read_destination<AB: AirBuilder, const W: usize>(
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
