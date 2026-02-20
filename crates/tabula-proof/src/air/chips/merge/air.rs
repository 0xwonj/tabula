//! GlobalMergeChip — AIR constraints for the 3-way merge table.
//!
//! The GlobalMerge table proves that OldList + WriteSet → NewList for each
//! touched SSMC column. Rows sorted by `(table_id, col_id, key)`.
//!
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (5): s1, s0, in_new, is_last_segment, is_first_in_new
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Key sorted uniqueness: within same segment, key_next > key
//! 4. Source encoding: derived selectors for old_only/write_only/both/delete
//! 5. Merge logic: new_val derivation + in_new correctness per source type
//! 6. Delete null witness: is_delete ⟹ write_val = 0^W (canonical null)
//! 7. Segment lex ordering: tc_changed detection via IsZero
//! 8. Hash accumulator carry: within same segment, deleted rows (in_new=0)
//!    must carry hash_acc forward unchanged (M9-A3)
//!
//! LogUp buses:
//! - C3 MergeOldList receive: `(t, c, key[3], old_val[W])`, mult = old-sourced rows
//! - C4 MergeWriteSet receive: `(t, c, key[3], write_val[W], is_delete)`, mult = write-sourced rows

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{CommitmentAirBuilder, MergeAirBuilder, PoseidonAirBuilder};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{
    constrain_hash_chain_input, constrain_hash_chain_transition, constrain_is_real_prefix,
    constrain_key_halves, constrain_lex_direction, constrain_ordering_halves,
    constrain_same_key_detection, constrain_strict_ineq, send_key_range_checks,
    send_lex_range_checks, send_ordering_range_checks,
};

use super::columns::{GlobalMergeCols, merge_width};

/// The GlobalMerge AIR chip, generic over value width.
#[derive(Debug)]
pub struct GlobalMergeChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for GlobalMergeChip<W> {
    fn width(&self) -> usize {
        merge_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for GlobalMergeChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &GlobalMergeCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &GlobalMergeCols<AB::Var, W> = borrow_cols(&next_row);

        let both_real: AB::Expr = local.is_real.clone().into() * next.is_real.clone().into();

        // ── 1. Boolean constraints ──
        builder.assert_bool(local.s1.clone());
        builder.assert_bool(local.s0.clone());
        builder.assert_bool(local.in_new.clone());
        builder.assert_bool(local.is_last_segment.clone());
        builder.assert_bool(local.is_first_in_new.clone());
        builder.assert_bool(local.has_prev_in_new.clone());

        // ── 2. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // ── 3. Same-key (t,c) detection ──
        {
            let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
            let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();
            constrain_same_key_detection(
                builder,
                &local.segment,
                table_diff,
                col_diff,
                both_real.clone(),
            );
        }

        // ── 4. Key sorted uniqueness ──
        {
            let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
            let mut when_transition = builder.when_transition();
            let mut when_both_real = when_transition.when(both_real.clone());
            let mut when_ordering = when_both_real.when(same_segment);
            constrain_strict_ineq(
                &mut when_ordering,
                &local.key.limbs,
                &next.key.limbs,
                &local.key_ordering.ineq,
            );
        }

        // ── 5-6. Merge logic + delete null ──
        constrain_merge_logic(builder, local);
        constrain_delete_null(builder, local);

        // ── 7. Hash acc carry ──
        constrain_hash_acc_carry(builder, local, next, both_real.clone());

        // ── 8. is_last_segment ──
        constrain_is_last_segment(builder, local, next, both_real.clone());

        // ── 9. is_first_in_new = in_new * (1 - has_prev_in_new) ──
        // Fully determines is_first_in_new: marks the first in_new=1 row per segment.
        builder.when(local.is_real.clone()).assert_zero(
            local.is_first_in_new.clone().into()
                - local.in_new.clone().into()
                    * (AB::Expr::ONE - local.has_prev_in_new.clone().into()),
        );

        // ── 9a. has_prev_in_new propagation ──
        constrain_has_prev_in_new(builder, local, next, both_real.clone());

        // ── 10. Hash chain input composition ──
        {
            let is_real: AB::Expr = local.is_real.clone().into();
            let is_first_in_new: AB::Expr = local.is_first_in_new.clone().into();
            let first_gate = is_real.clone() * is_first_in_new;
            let in_new: AB::Expr = local.in_new.clone().into();
            let not_first: AB::Expr = AB::Expr::ONE - local.is_first_in_new.clone().into();
            let cont_gate = is_real * in_new * not_first;
            constrain_hash_chain_input::<AB, W>(
                builder,
                &local.hash_chain,
                &local.key.limbs,
                &local.new_val,
                local.table_id.clone(),
                local.col_id.clone(),
                first_gate,
                cont_gate,
            );
            // Transition: next.perm_input[0..8] = local.hash_acc when next is continuation
            let trans_gate: AB::Expr = both_real.clone()
                * (AB::Expr::ONE - local.segment.tc_changed.clone().into())
                * next.in_new.clone().into()
                * (AB::Expr::ONE - next.is_first_in_new.clone().into());
            constrain_hash_chain_transition(
                builder,
                &next.hash_chain.perm_input,
                &local.hash_acc,
                trans_gate,
            );
        }

        // ── 11. Range check half-decomposition ──
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // ── 12. Lex ordering direction ──
        {
            let gate: AB::Expr = both_real * local.segment.tc_changed.clone().into();
            constrain_lex_direction(
                builder,
                &local.lex,
                next.table_id.clone().into(),
                local.table_id.clone().into(),
                next.col_id.clone().into(),
                local.col_id.clone().into(),
                gate,
            );
        }

        // ── LogUp buses ──
        let is_real: AB::Expr = local.is_real.clone().into();

        // C3 MergeOldList receive
        {
            let is_write_only: AB::Expr =
                (AB::Expr::ONE - local.s1.clone().into()) * local.s0.clone().into();
            builder.receive_merge_old_list(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.old_val,
                is_real.clone() * (AB::Expr::ONE - is_write_only),
            );
        }

        // C4 MergeWriteSet receive
        {
            let is_old_only: AB::Expr = (AB::Expr::ONE - local.s1.clone().into())
                * (AB::Expr::ONE - local.s0.clone().into());
            let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
            builder.receive_merge_write_set(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.write_val,
                is_delete,
                is_real.clone() * (AB::Expr::ONE - is_old_only),
            );
        }

        // C6 CommitmentVerification send (NewList commitment)
        builder.send_commitment(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            AB::Expr::ONE, // comm_type = 1 (Com_new)
            AB::Expr::ONE, // is_touched = 1 (Merge only for touched)
            &local.hash_acc,
            is_real.clone() * local.is_last_segment.clone().into(),
        );

        // C5 PoseidonPermutation send
        builder.send_poseidon_perm(
            &local.hash_chain.perm_input,
            &local.hash_acc,
            is_real.clone() * local.in_new.clone().into(),
        );

        // C8 RangeCheck sends
        send_key_range_checks(builder, &local.key, is_real.clone());
        send_ordering_range_checks(builder, &local.key_ordering, is_real.clone());
        {
            let tc: AB::Expr = local.segment.tc_changed.clone().into();
            send_lex_range_checks(builder, &local.lex, is_real * tc);
        }
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// 4-5. Merge logic: new_val and in_new correctness per source type.
fn constrain_merge_logic<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    let not_s1: AB::Expr = AB::Expr::ONE - local.s1.clone().into();
    let not_s0: AB::Expr = AB::Expr::ONE - local.s0.clone().into();
    let is_old_only: AB::Expr = not_s1.clone() * not_s0.clone();
    let is_write_only: AB::Expr = not_s1 * local.s0.clone().into();
    let is_both: AB::Expr = local.s1.clone().into() * not_s0;
    let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();

    // old_only: new_val = old_val, in_new = 1
    for i in 0..W {
        builder.when(local.is_real.clone()).assert_zero(
            is_old_only.clone()
                * (local.new_val[i].clone().into() - local.old_val[i].clone().into()),
        );
    }
    builder
        .when(local.is_real.clone())
        .assert_zero(is_old_only * (AB::Expr::ONE - local.in_new.clone().into()));

    // write_only: new_val = write_val, in_new = 1
    for i in 0..W {
        builder.when(local.is_real.clone()).assert_zero(
            is_write_only.clone()
                * (local.new_val[i].clone().into() - local.write_val[i].clone().into()),
        );
    }
    builder
        .when(local.is_real.clone())
        .assert_zero(is_write_only * (AB::Expr::ONE - local.in_new.clone().into()));

    // both: new_val = write_val, in_new = 1
    for i in 0..W {
        builder.when(local.is_real.clone()).assert_zero(
            is_both.clone() * (local.new_val[i].clone().into() - local.write_val[i].clone().into()),
        );
    }
    builder
        .when(local.is_real.clone())
        .assert_zero(is_both * (AB::Expr::ONE - local.in_new.clone().into()));

    // delete: in_new = 0
    builder
        .when(local.is_real.clone())
        .assert_zero(is_delete * local.in_new.clone().into());
}

/// 6. Delete null witness: is_delete ⟹ write_val = 0^W (canonical null).
fn constrain_delete_null<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
    for i in 0..W {
        builder
            .when(local.is_real.clone())
            .assert_zero(is_delete.clone() * local.write_val[i].clone().into());
    }
}

/// Hash accumulator carry: a non-`in_new` row inherits hash_acc unchanged.
fn constrain_hash_acc_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    let next_is_delete: AB::Expr = AB::Expr::ONE - next.in_new.clone().into();
    let gate: AB::Expr = both_real * same_segment * next_is_delete;

    for j in 0..8 {
        let diff: AB::Expr = next.hash_acc[j].clone().into() - local.hash_acc[j].clone().into();
        builder.when_transition().assert_zero(gate.clone() * diff);
    }
}

/// has_prev_in_new running flag: tracks whether any prior row in the same segment had in_new=1.
fn constrain_has_prev_in_new<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // First row: has_prev_in_new = 0 (nothing before first row)
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.has_prev_in_new.clone());

    let tc_changed: AB::Expr = local.segment.tc_changed.clone().into();
    let same_segment: AB::Expr = AB::Expr::ONE - tc_changed.clone();

    // On segment boundary: reset to 0
    builder.when_transition().assert_zero(
        both_real.clone() * tc_changed * next.has_prev_in_new.clone().into(),
    );

    // Within same segment: next.has_prev_in_new = local.has_prev_in_new OR local.in_new
    // = has_prev_in_new + in_new - has_prev_in_new * in_new
    let expected: AB::Expr = local.has_prev_in_new.clone().into()
        + local.in_new.clone().into()
        - local.has_prev_in_new.clone().into() * local.in_new.clone().into();
    builder.when_transition().assert_zero(
        both_real * same_segment * (next.has_prev_in_new.clone().into() - expected),
    );
}

/// is_last_segment: marks the last real row of each `(t,c)` segment.
fn constrain_is_last_segment<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    builder.when_transition().assert_zero(
        both_real
            * (local.is_last_segment.clone().into() - local.segment.tc_changed.clone().into()),
    );

    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last_segment.clone().into()));
}
