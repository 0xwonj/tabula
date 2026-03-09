//! StateShardChip — AIR constraints for per-column state commitment.
//!
//! Per-column version of `StateColumnChip`. Unifies SSMC (old commitment)
//! and Merge (old + write → new) with two parallel hash chains.
//!
//! Constraint groups:
//! 1. Boolean fields
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Constant identity: table_id, col_id same across all real rows
//! 4. Gap row canonicality
//! 5. Source encoding + merge logic
//! 6. Old hash chain (first/continuation/carry)
//! 7. New hash chain (first/continuation/carry)
//! 8. Key ordering (strict, unconditional between real rows)
//! 9. Chain tracking flags
//! 10. Touched-write closure (`segment_is_touched` <-> any write)
//!
//! LogUp buses:
//! - C13 BaseStateEntry receive
//! - C14 CoalescedWrite receive
//! - C5 PoseidonPerm send: old + new chains
//! - C6 CommitVerif send: Com_old, Com_new
//! - C8 RangeCheck sends

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_gadgets::{
    constrain_hash_chain_input, constrain_hash_chain_transition, constrain_is_real_prefix,
    constrain_key_halves, constrain_ordering_halves, constrain_strict_ineq,
};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{StateShardCols, state_shard_width};
use super::derived::{derive_in_new, derive_in_old, derive_in_write};

/// Per-column state shard AIR chip.
///
/// Each instance operates on a single `(table_id, col_id)` pair.
/// Maintains two parallel hash chains (Com_old, Com_new).
#[derive(Debug, Clone)]
pub struct StateShardChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<const W: usize> StateShardChip<W> {
    /// Create a new state shard chip for a specific column.
    pub fn new(chip_id: ChipId, table_id: u32, col_id: u16) -> Self {
        Self {
            chip_id,
            table_id,
            col_id,
        }
    }

    /// Table identifier this shard operates on.
    pub fn table_id(&self) -> u32 {
        self.table_id
    }

    /// Column identifier this shard operates on.
    pub fn col_id(&self) -> u16 {
        self.col_id
    }
}

impl<const W: usize> ChipSpec for StateShardChip<W> {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "StateShard"
    }
}

impl<F, const W: usize> BaseAir<F> for StateShardChip<W> {
    fn width(&self) -> usize {
        state_shard_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for StateShardChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &StateShardCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &StateShardCols<AB::Var, W> = borrow_cols(&next_row);

        let is_real: AB::Expr = local.is_real.clone().into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

        // 1. Boolean constraints
        constrain_booleans(builder, local);

        // 2. is_real prefix
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // 3. Constant identity
        constrain_constant_identity(builder, local, next, both_real.clone());

        // 4. Gap row canonicality
        constrain_gap_rows(builder, local, is_real.clone());

        // 5. Source encoding + merge logic
        constrain_merge_logic(builder, local, is_real.clone());

        // 6. Old hash chain
        let in_old = derive_in_old::<AB, W>(local);
        let in_new = derive_in_new::<AB, W>(local);
        constrain_old_hash_chain(builder, local, next, is_real.clone(), both_real.clone());

        // 7. New hash chain
        constrain_new_hash_chain(builder, local, next, is_real.clone(), both_real.clone());

        // 8. Key ordering (unconditional between all real rows)
        constrain_key_ordering(builder, local, next, both_real.clone());

        // 9. Chain tracking flags
        constrain_chain_tracking(builder, local, next, is_real.clone(), both_real.clone());

        // 10. Touched-write closure
        constrain_write_seen_prefix(builder, local, next, both_real.clone());
        constrain_segment_is_touched(builder, local, next, both_real.clone());
        constrain_touched_write_closure(builder, local, next, both_real.clone());

        // 11. Range check half-decomposition
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // LogUp buses
        super::buses::send_receive_buses(builder, local, is_real, in_old, in_new);
    }
}

// ── Constraint helpers ───────────────────────────────────────────────────────

fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
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

/// Constant identity: table_id and col_id must be the same across all real rows.
fn constrain_constant_identity<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
    let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real.clone() * table_diff);
    builder
        .when_transition()
        .assert_zero(both_real * col_diff);
}

/// Gap row canonicality: gap → source/values zero.
fn constrain_gap_rows<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
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

/// Source encoding + merge logic (for non-gap entry rows).
fn constrain_merge_logic<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let gate: AB::Expr = is_real * not_gap;
    let s1: AB::Expr = local.s1.clone().into();
    let s0: AB::Expr = local.s0.clone().into();
    let not_s1: AB::Expr = AB::Expr::ONE - s1.clone();
    let not_s0: AB::Expr = AB::Expr::ONE - s0.clone();

    let is_old_only: AB::Expr = not_s1.clone() * not_s0;
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

/// Old hash chain constraints (first/continuation/carry).
fn constrain_old_hash_chain<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
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
    let next_in_old = derive_in_old::<AB, W>(next);
    let next_has_prev: AB::Expr = next.has_prev_old_entry.clone().into();
    let trans_gate: AB::Expr = both_real.clone() * next_in_old * next_has_prev;
    constrain_hash_chain_transition(
        builder,
        &next.old_hash_chain.perm_input,
        &local.old_hash_acc,
        trans_gate,
    );

    // Carry: non-in_old rows carry old_hash_acc forward unchanged
    let not_in_old_next: AB::Expr = AB::Expr::ONE - derive_in_old::<AB, W>(next);
    let carry_gate: AB::Expr = both_real * not_in_old_next;
    for j in 0..8 {
        let diff: AB::Expr =
            next.old_hash_acc[j].clone().into() - local.old_hash_acc[j].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }
}

/// New hash chain constraints (first/continuation/carry).
fn constrain_new_hash_chain<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
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
    let next_in_new = derive_in_new::<AB, W>(next);
    let next_has_prev: AB::Expr = next.has_prev_new_entry.clone().into();
    let trans_gate: AB::Expr = both_real.clone() * next_in_new * next_has_prev;
    constrain_hash_chain_transition(
        builder,
        &next.new_hash_chain.perm_input,
        &local.new_hash_acc,
        trans_gate,
    );

    // Carry: non-in_new rows carry new_hash_acc forward unchanged
    let not_in_new_next: AB::Expr = AB::Expr::ONE - derive_in_new::<AB, W>(next);
    let carry_gate: AB::Expr = both_real * not_in_new_next;
    for j in 0..8 {
        let diff: AB::Expr =
            next.new_hash_acc[j].clone().into() - local.new_hash_acc[j].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }
}

/// Key ordering: strict between all consecutive real rows.
fn constrain_key_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let mut when_transition = builder.when_transition();
    let mut when_both_real = when_transition.when(both_real);
    constrain_strict_ineq(
        &mut when_both_real,
        &local.key.limbs,
        &next.key.limbs,
        &local.key_ordering.ineq,
    );
}

/// Chain tracking flag propagation.
fn constrain_chain_tracking<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
) {
    let in_old = derive_in_old::<AB, W>(local);
    let in_new = derive_in_new::<AB, W>(local);

    // First row constraints: no prior entries
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

    // has_prev_old_entry propagation: next.has_prev = local.has_prev OR in_old
    let expected_has_prev_old: AB::Expr = local.has_prev_old_entry.clone().into() + in_old.clone()
        - local.has_prev_old_entry.clone().into() * in_old.clone();
    builder.when_transition().assert_zero(
        both_real.clone()
            * (next.has_prev_old_entry.clone().into() - expected_has_prev_old),
    );

    // has_prev_new_entry propagation
    let expected_has_prev_new: AB::Expr = local.has_prev_new_entry.clone().into() + in_new.clone()
        - local.has_prev_new_entry.clone().into() * in_new.clone();
    builder.when_transition().assert_zero(
        both_real.clone()
            * (next.has_prev_new_entry.clone().into() - expected_has_prev_new),
    );

    // is_last_old_entry implies in_old
    builder.assert_zero(
        is_real.clone() * local.is_last_old_entry.clone().into() * (AB::Expr::ONE - in_old.clone()),
    );

    // is_last_new_entry implies in_new
    builder.assert_zero(
        is_real.clone() * local.is_last_new_entry.clone().into() * (AB::Expr::ONE - in_new),
    );

    // past_last_old_entry propagation: next.past_last = local.past_last OR local.is_last_old
    let expected_past_last: AB::Expr = local.past_last_old_entry.clone().into()
        + local.is_last_old_entry.clone().into()
        - local.past_last_old_entry.clone().into() * local.is_last_old_entry.clone().into();
    builder.when_transition().assert_zero(
        both_real.clone()
            * (next.past_last_old_entry.clone().into() - expected_past_last),
    );

    // past_last_old → no more in_old
    builder.assert_zero(is_real * local.past_last_old_entry.clone().into() * in_old);

    // Completeness at end: if had old entries, must have is_last_old or past_last_old
    let in_old_here = derive_in_old::<AB, W>(local);
    let had_old: AB::Expr = local.has_prev_old_entry.clone().into() + in_old_here.clone()
        - local.has_prev_old_entry.clone().into() * in_old_here;
    let covered: AB::Expr = local.is_last_old_entry.clone().into()
        + local.past_last_old_entry.clone().into()
        - local.is_last_old_entry.clone().into() * local.past_last_old_entry.clone().into();

    // At real→padding boundary
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * had_old * (AB::Expr::ONE - covered));
}

/// Running write-seen prefix.
fn constrain_write_seen_prefix<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let local_seen: AB::Expr = local.write_seen_prefix.clone().into();
    let next_seen: AB::Expr = next.write_seen_prefix.clone().into();
    let next_write: AB::Expr = derive_in_write::<AB, W>(next);

    // First row initializes the accumulator.
    let local_write: AB::Expr = derive_in_write::<AB, W>(local);
    builder
        .when_first_row()
        .assert_zero(local_seen.clone() - local_write);

    // Propagation: next_seen = local_seen OR next_write
    let seen_or_next: AB::Expr =
        local_seen.clone() + next_write.clone() - local_seen * next_write;
    builder
        .when_transition()
        .assert_zero(both_real * (next_seen - seen_or_next));
}

/// `segment_is_touched` constancy across all rows.
fn constrain_segment_is_touched<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let diff: AB::Expr =
        next.segment_is_touched.clone().into() - local.segment_is_touched.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real * diff);
}

/// Touched-write closure at end of real rows.
fn constrain_touched_write_closure<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    next: &StateShardCols<AB::Var, W>,
    _both_real: AB::Expr,
) {
    let touched_diff: AB::Expr =
        local.segment_is_touched.clone().into() - local.write_seen_prefix.clone().into();

    // At real→padding boundary
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * touched_diff);
}
