//! StateColumnChip — AIR constraints for the unified state column table.
//!
//! Replaces GlobalSSMC + GlobalMerge with a single chip maintaining two
//! parallel hash chains (Com_old and Com_new) over sorted entries.
//!
//! Constraint groups:
//! 1. Boolean fields
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Gap row canonicality
//! 4. Source encoding + merge logic
//! 5. Old hash chain (first/continuation/carry)
//! 6. New hash chain (first/continuation/carry)
//! 7. Key ordering (strict, within segment)
//! 8. Segment detection + lex ordering
//! 9. Chain tracking flags
//! 10. segment_is_touched constancy
//!
//! LogUp buses:
//! - C13 BaseStateEntry receive: in_old + gap + write_only rows (from InterTxOrder)
//! - C14 CoalescedWrite receive: write entries (from InterTxOrder)
//! - C5 PoseidonPerm send: old + new chains
//! - C6 CommitVerif send: Com_old, Com_new
//! - C8 RangeCheck sends

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{
    BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, CommitmentAirBuilder, PoseidonAirBuilder,
};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{
    constrain_hash_chain_input, constrain_hash_chain_transition, constrain_is_real_prefix,
    constrain_key_halves, constrain_lex_direction, constrain_ordering_halves,
    constrain_same_key_detection, constrain_strict_ineq, send_key_range_checks,
    send_lex_range_checks, send_ordering_range_checks,
};

use super::columns::{StateColumnCols, state_column_width};

/// The StateColumn AIR chip, generic over value width.
#[derive(Debug)]
pub struct StateColumnChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for StateColumnChip<W> {
    fn width(&self) -> usize {
        state_column_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for StateColumnChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &StateColumnCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &StateColumnCols<AB::Var, W> = borrow_cols(&next_row);

        let is_real: AB::Expr = local.is_real.clone().into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

        // ── 1. Boolean constraints ──
        constrain_booleans(builder, local);

        // ── 2. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // ── 3. Gap row canonicality ──
        constrain_gap_rows(builder, local, is_real.clone());

        // ── 4. Source encoding + merge logic ──
        constrain_merge_logic(builder, local, is_real.clone());

        // ── 5-6. Hash chains ──
        let in_old = derive_in_old::<AB, W>(local);
        let in_new = derive_in_new::<AB, W>(local);

        constrain_old_hash_chain(builder, local, next, is_real.clone(), both_real.clone());
        constrain_new_hash_chain(builder, local, next, is_real.clone(), both_real.clone());

        // ── 7. Key ordering ──
        constrain_key_ordering(builder, local, next, both_real.clone());

        // ── 8. Segment detection + lex ordering ──
        constrain_segment_and_lex(builder, local, next, both_real.clone());

        // ── 9. Chain tracking flags ──
        constrain_chain_tracking(builder, local, next, is_real.clone(), both_real.clone());

        // ── 10. segment_is_touched constancy ──
        constrain_segment_is_touched(builder, local, next, both_real.clone());

        // ── 11. Range check half-decomposition ──
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // ── LogUp buses ──

        // C13 BaseStateEntry receive: in_old entries + gap rows + write_only (from InterTxOrder)
        {
            let is_write_only = derive_is_write_only::<AB, W>(local);
            // In-old entries (old_only, both, delete) → (t, c, key, old_val, 0)
            builder.receive_base_state_entry(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.old_val,
                AB::Expr::ZERO,
                is_real.clone() * in_old.clone() * local.read_mult_witness.clone().into(),
            );
            // Gap rows → (t, c, key, zeros, 1)
            builder.receive_base_state_entry(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.new_val, // zeros for gap rows (constrained)
                AB::Expr::ONE,
                is_real.clone()
                    * local.is_gap.clone().into()
                    * local.read_mult_witness.clone().into(),
            );
            // Write-only entries → (t, c, key, zeros=old_val, 1)
            builder.receive_base_state_entry(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.old_val, // zeros for write_only (constrained by merge logic)
                AB::Expr::ONE,
                is_real.clone() * is_write_only * local.read_mult_witness.clone().into(),
            );
        }

        // C14 CoalescedWrite receive: write entries (write_only, both, delete) (from InterTxOrder)
        {
            let in_write = derive_in_write::<AB, W>(local);
            let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
            builder.receive_coalesced_write(
                local.table_id.clone().into(),
                local.col_id.clone().into(),
                &local.key.limbs,
                &local.new_val,
                is_delete,
                is_real.clone() * in_write * local.write_mult_witness.clone().into(),
            );
        }

        // C5 PoseidonPerm send: old chain
        builder.send_poseidon_perm(
            &local.old_hash_chain.perm_input,
            &local.old_hash_acc,
            is_real.clone() * in_old,
        );

        // C5 PoseidonPerm send: new chain
        builder.send_poseidon_perm(
            &local.new_hash_chain.perm_input,
            &local.new_hash_acc,
            is_real.clone() * in_new,
        );

        // C6 CommitmentVerification send: Com_old at segment end
        builder.send_commitment(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            AB::Expr::ZERO, // comm_type = 0 (Com_old)
            local.segment_is_touched.clone().into(),
            &local.old_hash_acc,
            is_real.clone() * local.is_last_old_entry.clone().into(),
        );

        // C6 CommitmentVerification send: Com_new at segment end (only if touched)
        builder.send_commitment(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            AB::Expr::ONE, // comm_type = 1 (Com_new)
            AB::Expr::ONE,
            &local.new_hash_acc,
            is_real.clone()
                * local.is_last_new_entry.clone().into()
                * local.segment_is_touched.clone().into(),
        );

        // C8 RangeCheck sends
        send_key_range_checks(builder, &local.key, is_real.clone());
        {
            let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
            send_ordering_range_checks(
                builder,
                &local.key_ordering,
                is_real.clone() * same_segment,
            );
        }
        {
            let tc: AB::Expr = local.segment.tc_changed.clone().into();
            send_lex_range_checks(builder, &local.lex, is_real * tc);
        }
    }
}

// ── Derived flag expressions ─────────────────────────────────────────────────

/// `in_old = !is_gap * (1 - (1-s1)*s0)` — old_only, both, delete.
fn derive_in_old<AB: AirBuilder, const W: usize>(local: &StateColumnCols<AB::Var, W>) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    // in_old = !is_gap * (1 - s0 + s1*s0)
    let s0: AB::Expr = local.s0.clone().into();
    let s1: AB::Expr = local.s1.clone().into();
    not_gap * (AB::Expr::ONE - s0.clone() + s1 * s0)
}

/// `in_new = !is_gap * (1 - s1*s0)` — old_only, write_only, both.
fn derive_in_new<AB: AirBuilder, const W: usize>(local: &StateColumnCols<AB::Var, W>) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let s1_s0: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
    not_gap * (AB::Expr::ONE - s1_s0)
}

/// `is_write_only = !is_gap * !s1 * s0` — write_only only.
fn derive_is_write_only<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let not_s1: AB::Expr = AB::Expr::ONE - local.s1.clone().into();
    not_gap * not_s1 * local.s0.clone().into()
}

/// `in_write = !is_gap * (s0 + s1 - s0*s1)` — write_only, both, delete.
fn derive_in_write<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let s0: AB::Expr = local.s0.clone().into();
    let s1: AB::Expr = local.s1.clone().into();
    not_gap * (s0.clone() + s1.clone() - s0 * s1)
}

// ── Constraint helpers ───────────────────────────────────────────────────────

/// 1. Boolean constraints on all flag columns.
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_gap.clone());
    builder.assert_bool(local.s1.clone());
    builder.assert_bool(local.s0.clone());
    builder.assert_bool(local.segment_is_touched.clone());
    builder.assert_bool(local.has_prev_old_entry.clone());
    builder.assert_bool(local.is_last_old_entry.clone());
    builder.assert_bool(local.past_last_old_entry.clone());
    builder.assert_bool(local.has_prev_new_entry.clone());
    builder.assert_bool(local.is_last_new_entry.clone());
    builder.assert_bool(local.read_mult_witness.clone());
    builder.assert_bool(local.write_mult_witness.clone());
}

/// 3. Gap row canonicality: gap → source/values zero.
fn constrain_gap_rows<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gap: AB::Expr = local.is_gap.clone().into();
    let gate: AB::Expr = is_real * gap;

    builder.assert_zero(gate.clone() * local.s1.clone().into());
    builder.assert_zero(gate.clone() * local.s0.clone().into());

    for i in 0..W {
        builder.assert_zero(gate.clone() * local.old_val[i].clone().into());
        builder.assert_zero(gate.clone() * local.new_val[i].clone().into());
    }
}

/// 4. Source encoding + merge logic (for non-gap entry rows).
fn constrain_merge_logic<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let gate: AB::Expr = is_real * not_gap;
    let s1: AB::Expr = local.s1.clone().into();
    let s0: AB::Expr = local.s0.clone().into();
    let not_s1: AB::Expr = AB::Expr::ONE - s1.clone();
    let not_s0: AB::Expr = AB::Expr::ONE - s0.clone();

    let is_old_only: AB::Expr = not_s1.clone() * not_s0.clone();
    let is_write_only: AB::Expr = not_s1 * s0.clone();
    let is_delete: AB::Expr = s1 * s0;

    // old_only: new_val = old_val
    for i in 0..W {
        builder.assert_zero(
            gate.clone()
                * is_old_only.clone()
                * (local.new_val[i].clone().into() - local.old_val[i].clone().into()),
        );
    }

    // write_only: old_val = 0 (canonical)
    for i in 0..W {
        builder.assert_zero(gate.clone() * is_write_only.clone() * local.old_val[i].clone().into());
    }

    // delete: new_val = 0 (canonical null in new set)
    for i in 0..W {
        builder.assert_zero(gate.clone() * is_delete.clone() * local.new_val[i].clone().into());
    }
}

/// 5. Old hash chain constraints.
fn constrain_old_hash_chain<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
) {
    let in_old = derive_in_old::<AB, W>(local);
    let is_first_old: AB::Expr =
        in_old.clone() * (AB::Expr::ONE - local.has_prev_old_entry.clone().into());
    let is_cont_old: AB::Expr = in_old * local.has_prev_old_entry.clone().into();

    let first_gate: AB::Expr = is_real.clone() * is_first_old;
    let cont_gate: AB::Expr = is_real * is_cont_old;

    constrain_hash_chain_input::<AB, W>(
        builder,
        &local.old_hash_chain,
        &local.key.limbs,
        &local.old_val,
        local.table_id.clone(),
        local.col_id.clone(),
        first_gate,
        cont_gate,
    );

    // Transition: link prev old_hash_acc into next old_hash_chain.perm_input[0..8]
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    let next_in_old = derive_in_old::<AB, W>(next);
    let next_has_prev: AB::Expr = next.has_prev_old_entry.clone().into();
    let trans_gate: AB::Expr =
        both_real.clone() * same_segment.clone() * next_in_old * next_has_prev;
    constrain_hash_chain_transition(
        builder,
        &next.old_hash_chain.perm_input,
        &local.old_hash_acc,
        trans_gate,
    );

    // Carry: non-in_old rows carry old_hash_acc forward unchanged within segment
    let not_in_old_next: AB::Expr = AB::Expr::ONE - derive_in_old::<AB, W>(next);
    let carry_gate: AB::Expr = both_real * same_segment * not_in_old_next;
    for j in 0..8 {
        let diff: AB::Expr =
            next.old_hash_acc[j].clone().into() - local.old_hash_acc[j].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }
}

/// 6. New hash chain constraints.
fn constrain_new_hash_chain<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
) {
    let in_new = derive_in_new::<AB, W>(local);
    let is_first_new: AB::Expr =
        in_new.clone() * (AB::Expr::ONE - local.has_prev_new_entry.clone().into());
    let is_cont_new: AB::Expr = in_new * local.has_prev_new_entry.clone().into();

    let first_gate: AB::Expr = is_real.clone() * is_first_new;
    let cont_gate: AB::Expr = is_real * is_cont_new;

    constrain_hash_chain_input::<AB, W>(
        builder,
        &local.new_hash_chain,
        &local.key.limbs,
        &local.new_val,
        local.table_id.clone(),
        local.col_id.clone(),
        first_gate,
        cont_gate,
    );

    // Transition: link prev new_hash_acc into next new_hash_chain.perm_input[0..8]
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    let next_in_new = derive_in_new::<AB, W>(next);
    let next_has_prev: AB::Expr = next.has_prev_new_entry.clone().into();
    let trans_gate: AB::Expr =
        both_real.clone() * same_segment.clone() * next_in_new * next_has_prev;
    constrain_hash_chain_transition(
        builder,
        &next.new_hash_chain.perm_input,
        &local.new_hash_acc,
        trans_gate,
    );

    // Carry: non-in_new rows carry new_hash_acc forward unchanged within segment
    let not_in_new_next: AB::Expr = AB::Expr::ONE - derive_in_new::<AB, W>(next);
    let carry_gate: AB::Expr = both_real * same_segment * not_in_new_next;
    for j in 0..8 {
        let diff: AB::Expr =
            next.new_hash_acc[j].clone().into() - local.new_hash_acc[j].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }
}

/// 7. Key ordering: strict within same segment.
fn constrain_key_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    let mut when_transition = builder.when_transition();
    let mut when_both_real = when_transition.when(both_real);
    let mut when_ordering = when_both_real.when(same_segment);
    constrain_strict_ineq(
        &mut when_ordering,
        &local.key.limbs,
        &next.key.limbs,
        &local.key_ordering.ineq,
    );
}

/// 8. Segment detection + lex ordering direction.
fn constrain_segment_and_lex<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
    let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();
    constrain_same_key_detection(
        builder,
        &local.segment,
        table_diff,
        col_diff,
        both_real.clone(),
    );

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

/// 9. Chain tracking flag propagation.
fn constrain_chain_tracking<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
) {
    let tc_changed: AB::Expr = local.segment.tc_changed.clone().into();
    let same_segment: AB::Expr = AB::Expr::ONE - tc_changed.clone();

    let in_old = derive_in_old::<AB, W>(local);
    let in_new = derive_in_new::<AB, W>(local);

    // ── First row constraints ──
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.has_prev_old_entry.clone());
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.has_prev_new_entry.clone());
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(local.past_last_old_entry.clone());

    // ── Segment boundary reset ──
    builder.when_transition().assert_zero(
        both_real.clone() * tc_changed.clone() * next.has_prev_old_entry.clone().into(),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * tc_changed.clone() * next.has_prev_new_entry.clone().into(),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * tc_changed.clone() * next.past_last_old_entry.clone().into(),
    );

    // ── has_prev_old_entry propagation ──
    // next.has_prev = local.has_prev OR in_old
    let expected_has_prev_old: AB::Expr = local.has_prev_old_entry.clone().into() + in_old.clone()
        - local.has_prev_old_entry.clone().into() * in_old.clone();
    builder.when_transition().assert_zero(
        both_real.clone()
            * same_segment.clone()
            * (next.has_prev_old_entry.clone().into() - expected_has_prev_old),
    );

    // ── has_prev_new_entry propagation ──
    let expected_has_prev_new: AB::Expr = local.has_prev_new_entry.clone().into() + in_new.clone()
        - local.has_prev_new_entry.clone().into() * in_new.clone();
    builder.when_transition().assert_zero(
        both_real.clone()
            * same_segment.clone()
            * (next.has_prev_new_entry.clone().into() - expected_has_prev_new),
    );

    // ── is_last_old_entry implies in_old ──
    builder.assert_zero(
        is_real.clone() * local.is_last_old_entry.clone().into() * (AB::Expr::ONE - in_old.clone()),
    );

    // ── is_last_new_entry implies in_new ──
    builder.assert_zero(
        is_real.clone() * local.is_last_new_entry.clone().into() * (AB::Expr::ONE - in_new),
    );

    // ── past_last_old_entry propagation ──
    // Within segment: next.past_last = local.past_last OR local.is_last_old
    let expected_past_last: AB::Expr = local.past_last_old_entry.clone().into()
        + local.is_last_old_entry.clone().into()
        - local.past_last_old_entry.clone().into() * local.is_last_old_entry.clone().into();
    builder.when_transition().assert_zero(
        both_real.clone()
            * same_segment
            * (next.past_last_old_entry.clone().into() - expected_past_last),
    );

    // ── past_last_old → no more in_old ──
    builder.assert_zero(is_real * local.past_last_old_entry.clone().into() * in_old);

    // ── Completeness at segment end ──
    // At segment boundary: if segment had old entries, must have is_last_old or past_last_old.
    let in_old_here = derive_in_old::<AB, W>(local);
    let had_old: AB::Expr = local.has_prev_old_entry.clone().into() + in_old_here.clone()
        - local.has_prev_old_entry.clone().into() * in_old_here;
    let covered: AB::Expr = local.is_last_old_entry.clone().into()
        + local.past_last_old_entry.clone().into()
        - local.is_last_old_entry.clone().into() * local.past_last_old_entry.clone().into();

    // At tc_changed boundary
    builder.when_transition().assert_zero(
        both_real.clone() * tc_changed * had_old.clone() * (AB::Expr::ONE - covered.clone()),
    );

    // At real→padding boundary
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * had_old * (AB::Expr::ONE - covered));
}

/// 10. segment_is_touched constancy within segment.
fn constrain_segment_is_touched<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    let diff: AB::Expr =
        next.segment_is_touched.clone().into() - local.segment_is_touched.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real * same_segment * diff);
}
