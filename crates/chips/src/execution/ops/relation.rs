//! AIR constraints for the RelationProof opcode.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use super::super::columns::{ExecutionCols, MAX_SLOTS};

/// Constrain the RelationProof opcode.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn constrain_relation_table<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real.clone() * local.op_relation_table.into();

    builder.assert_zero(
        gate.clone()
            * local.relation_is_eval.into()
            * (AB::Expr::ONE - local.relation_is_eval.into()),
    );

    let written_sum: AB::Expr = (0..MAX_SLOTS).map(|s| local.slot_written[s].into()).sum();
    let output_used_sum: AB::Expr = (0..MAX_SLOTS)
        .map(|idx| local.relation_output_used[idx].into())
        .sum();

    builder.assert_zero(gate.clone() * (written_sum.clone() - output_used_sum.clone()));
    builder.assert_zero(
        gate.clone() * (AB::Expr::ONE - local.relation_is_eval.into()) * output_used_sum.clone(),
    );

    for idx in 0..MAX_SLOTS {
        builder.assert_zero(
            gate.clone()
                * local.relation_input_used[idx].into()
                * (AB::Expr::ONE - local.relation_input_used[idx].into()),
        );
        builder.assert_zero(
            gate.clone()
                * local.relation_output_used[idx].into()
                * (AB::Expr::ONE - local.relation_output_used[idx].into()),
        );
    }
    for idx in 0..MAX_SLOTS - 1 {
        builder.assert_zero(
            gate.clone()
                * local.relation_input_used[idx + 1].into()
                * (AB::Expr::ONE - local.relation_input_used[idx].into()),
        );
        builder.assert_zero(
            gate.clone()
                * local.relation_output_used[idx + 1].into()
                * (AB::Expr::ONE - local.relation_output_used[idx].into()),
        );
    }

    for tuple_idx in 0..MAX_SLOTS {
        let input_used: AB::Expr = local.relation_input_used[tuple_idx].into();
        let output_used: AB::Expr = local.relation_output_used[tuple_idx].into();

        let input_sel_sum: AB::Expr = (0..MAX_SLOTS)
            .map(|slot| local.relation_input_sel[tuple_idx][slot].into())
            .sum();
        let output_sel_sum: AB::Expr = (0..MAX_SLOTS)
            .map(|slot| local.relation_output_sel[tuple_idx][slot].into())
            .sum();

        builder.assert_zero(gate.clone() * (input_sel_sum - input_used.clone()));
        builder.assert_zero(gate.clone() * (output_sel_sum - output_used.clone()));

        for slot in 0..MAX_SLOTS {
            let input_sel: AB::Expr = local.relation_input_sel[tuple_idx][slot].into();
            let output_sel: AB::Expr = local.relation_output_sel[tuple_idx][slot].into();

            builder.assert_zero(
                gate.clone()
                    * input_sel.clone()
                    * (AB::Expr::ONE - local.relation_input_sel[tuple_idx][slot].into()),
            );
            builder.assert_zero(
                gate.clone()
                    * output_sel.clone()
                    * (AB::Expr::ONE - local.relation_output_sel[tuple_idx][slot].into()),
            );

            builder.assert_zero(
                gate.clone()
                    * output_sel.clone()
                    * (AB::Expr::ONE - local.slot_written[slot].into()),
            );

            for limb in 0..W {
                builder.assert_zero(
                    gate.clone()
                        * input_sel.clone()
                        * (local.relation_input_vals[tuple_idx][limb].into()
                            - local.slots[slot][limb].into()),
                );
                builder.assert_zero(
                    gate.clone()
                        * output_sel.clone()
                        * (local.relation_output_vals[tuple_idx][limb].into()
                            - local.slots[slot][limb].into()),
                );
            }
            builder.assert_zero(gate.clone() * input_sel * local.slot_is_null[slot].into());
            builder.assert_zero(gate.clone() * output_sel * local.slot_is_null[slot].into());
        }
    }

    for slot in 0..MAX_SLOTS {
        let output_cover: AB::Expr = (0..MAX_SLOTS)
            .map(|tuple_idx| local.relation_output_sel[tuple_idx][slot].into())
            .sum();
        builder
            .assert_zero(gate.clone() * (local.slot_written[slot].into() - output_cover.clone()));
        for lhs in 0..MAX_SLOTS {
            for rhs in lhs + 1..MAX_SLOTS {
                builder.assert_zero(
                    gate.clone()
                        * local.relation_output_sel[lhs][slot].into()
                        * local.relation_output_sel[rhs][slot].into(),
                );
            }
        }
    }
}
