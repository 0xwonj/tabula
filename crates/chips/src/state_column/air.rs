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
//! 10. Touched-write closure (`segment_is_touched` <-> any write in segment)
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

use tabula_gadgets::{
    constrain_hash_chain_input, constrain_hash_chain_transition, constrain_is_real_prefix,
    constrain_key_halves, constrain_lex_direction, constrain_ordering_halves,
    constrain_same_key_detection, constrain_strict_ineq,
};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;

use super::columns::{StateColumnCols, state_column_width};
use super::derived::{derive_in_new, derive_in_old, derive_in_write};

/// The StateColumn AIR chip, generic over value width.
#[derive(Clone, Copy, Debug)]
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

        // ── 10. Touched-write closure ──
        constrain_write_seen_prefix(builder, local, next, both_real.clone());
        constrain_segment_is_touched(builder, local, next, both_real.clone());
        constrain_touched_write_closure(builder, local, next, both_real.clone());

        // ── 11. Range check half-decomposition ──
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // ── LogUp buses ──
        super::buses::send_receive_buses(builder, local, is_real, in_old, in_new);
    }
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
    builder.assert_bool(local.write_seen_prefix.clone());
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

/// 10a. Running write-seen prefix within segment.
///
/// `write_seen_prefix` is an OR accumulator of `in_write`:
/// - first row of trace: `write_seen_prefix = in_write`
/// - same segment: `next = local OR next.in_write`
/// - new segment: `next = next.in_write`
fn constrain_write_seen_prefix<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let local_seen: AB::Expr = local.write_seen_prefix.clone().into();
    let next_seen: AB::Expr = next.write_seen_prefix.clone().into();
    let next_write: AB::Expr = derive_in_write::<AB, W>(next);
    let tc_changed: AB::Expr = local.segment.tc_changed.clone().into();
    let same_segment: AB::Expr = AB::Expr::ONE - tc_changed.clone();

    // First row initializes the accumulator.
    let local_write: AB::Expr = derive_in_write::<AB, W>(local);
    builder
        .when_first_row()
        .assert_zero(local_seen.clone() - local_write);

    // Same segment: next_seen = local_seen OR next_write.
    let seen_or_next: AB::Expr =
        local_seen.clone() + next_write.clone() - local_seen * next_write.clone();
    builder
        .when_transition()
        .assert_zero(both_real.clone() * same_segment * (next_seen.clone() - seen_or_next));

    // New segment: accumulator resets to next_write.
    builder
        .when_transition()
        .assert_zero(both_real * tc_changed * (next_seen - next_write));
}

/// 10b. `segment_is_touched` constancy within segment.
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

/// 10c. Touched-write closure at segment end.
///
/// At each segment boundary, `segment_is_touched` must equal
/// `write_seen_prefix` accumulated for that segment.
fn constrain_touched_write_closure<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    next: &StateColumnCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let touched_diff: AB::Expr =
        local.segment_is_touched.clone().into() - local.write_seen_prefix.clone().into();
    let tc_changed: AB::Expr = local.segment.tc_changed.clone().into();

    // Segment boundary between real rows.
    builder
        .when_transition()
        .assert_zero(both_real.clone() * tc_changed * touched_diff.clone());

    // Final real row before padding.
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * touched_diff);
}
