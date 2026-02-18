//! GlobalMergeChip — AIR constraints for the 3-way merge table.
//!
//! The GlobalMerge table proves that OldList + WriteSet → NewList for each
//! touched SSMC column. Rows sorted by `(table_id, col_id, key)`.
//!
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (5): is_real, s1, s0, in_new, tc_changed
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
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero, constrain_strict_ineq};
use crate::air::interaction::{AirInteraction, InteractionKind};

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

        constrain_booleans(builder, local);
        constrain_is_real(builder, local, next);
        constrain_same_key_detection(builder, local, next, both_real.clone());
        constrain_key_ordering(builder, local, next, both_real.clone());
        constrain_merge_logic(builder, local);
        constrain_delete_null(builder, local);
        constrain_hash_acc_carry(builder, local, next, both_real.clone());
        constrain_is_last_segment(builder, local, next, both_real.clone());
        constrain_is_first_in_new(builder, local);
        constrain_merge_hash_chain_input(builder, local, next, both_real);

        // ── LogUp buses ──
        receive_merge_old_list(builder, local);
        receive_merge_write_set(builder, local);
        send_commitment_verification(builder, local);
        send_poseidon_permutation(builder, local);
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// 1. Boolean constraints on flag columns (is_real handled by is_real_prefix).
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    builder.assert_bool(local.s1.clone());
    builder.assert_bool(local.s0.clone());
    builder.assert_bool(local.in_new.clone());
    builder.assert_bool(local.tc_changed.clone());
    builder.assert_bool(local.is_last_segment.clone());
    builder.assert_bool(local.is_first_in_new.clone());
}

/// 2. `is_real` prefix: monotonic 1→0 transition.
fn constrain_is_real<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
) {
    constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
}

/// 7. Same-key detection via IsZero + tc_changed derivation.
fn constrain_same_key_detection<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
    let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();

    constrain_is_zero(builder, table_diff, &local.table_diff_iz);
    constrain_is_zero(builder, col_diff, &local.col_diff_iz);

    let table_same: AB::Expr = local.table_diff_iz.is_zero.clone().into();
    let col_same: AB::Expr = local.col_diff_iz.is_zero.clone().into();
    let expected_tc_changed: AB::Expr = AB::Expr::ONE - table_same * col_same;
    builder
        .when_transition()
        .assert_zero(both_real * (local.tc_changed.clone().into() - expected_tc_changed));
}

/// 3. Key sorted uniqueness: within same segment, key_next > key.
fn constrain_key_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();

    let mut when_transition = builder.when_transition();
    let mut when_both_real = when_transition.when(both_real);
    let mut when_ordering = when_both_real.when(same_segment);
    constrain_strict_ineq(
        &mut when_ordering,
        &local.key,
        &next.key,
        &local.key_ordering,
    );
}

/// 4-5. Merge logic: new_val and in_new correctness per source type.
fn constrain_merge_logic<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    // Derived source selectors:
    //   is_old_only   = (1-s1)(1-s0)
    //   is_write_only = (1-s1)·s0
    //   is_both       = s1·(1-s0)
    //   is_delete     = s1·s0
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
///
/// Gated on `(1 − next.in_new)`: when the NEXT row is a delete, its hash_acc
/// must equal the current row's hash_acc. When the next row has `in_new=1`,
/// it computes its own hash_acc via the Poseidon hash chain.
///
/// `both_real · (1 − tc_changed) · (1 − next.in_new) · (next.hash_acc[j] − local.hash_acc[j]) = 0`
fn constrain_hash_acc_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();
    let next_is_delete: AB::Expr = AB::Expr::ONE - next.in_new.clone().into();
    let gate: AB::Expr = both_real * same_segment * next_is_delete;

    for j in 0..8 {
        let diff: AB::Expr = next.hash_acc[j].clone().into() - local.hash_acc[j].clone().into();
        builder.when_transition().assert_zero(gate.clone() * diff);
    }
}

/// is_last_segment: marks the last real row of each `(t,c)` segment.
///
/// Same pattern as SSMC's `is_last`:
/// - Within real transitions: is_last_segment = tc_changed
/// - Real-to-padding: is_last_segment must be 1
fn constrain_is_last_segment<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // Within real transitions: is_last_segment = tc_changed.
    builder.when_transition().assert_zero(
        both_real * (local.is_last_segment.clone().into() - local.tc_changed.clone().into()),
    );

    // Real-to-padding: is_last_segment must be 1.
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last_segment.clone().into()));
}

/// `is_first_in_new` implies `in_new`.
///
/// A row can only be the "first in new" if it's actually in the new list.
fn constrain_is_first_in_new<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    // is_first_in_new · (1 − in_new) = 0
    builder.when(local.is_real.clone()).assert_zero(
        local.is_first_in_new.clone().into() * (AB::Expr::ONE - local.in_new.clone().into()),
    );
}

/// Merge hash chain input composition constraints (C5 prerequisite).
///
/// **First-in-new** (`is_first_in_new=1`, `in_new=1`):
/// ```text
/// perm_input = [0x00, table_id, col_id, key[3], new_val[W], 0..]
/// ```
///
/// **Continuation** (`is_first_in_new=0`, `in_new=1`): local constraints for
/// key/value slots, plus transition linking `perm_input[0..8]` to prev `hash_acc`.
/// ```text
/// perm_input = [prev_hash_acc[8], key[3], new_val[W], 0..]
/// ```
///
/// Rows with `in_new=0` don't participate in hashing (mult=0 on C5 send),
/// so their `perm_input` is unconstrained.
fn constrain_merge_hash_chain_input<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let in_new: AB::Expr = local.in_new.clone().into();
    let is_first_in_new: AB::Expr = local.is_first_in_new.clone().into();
    let not_first_in_new: AB::Expr = AB::Expr::ONE - is_first_in_new.clone();

    // ── First-in-new composition ──
    let first_gate: AB::Expr = local.is_real.clone().into() * is_first_in_new;
    // perm_input[0] = 0 (domain tag)
    builder.assert_zero(first_gate.clone() * local.perm_input[0].clone().into());
    // perm_input[1] = table_id
    builder.assert_zero(
        first_gate.clone() * (local.perm_input[1].clone().into() - local.table_id.clone().into()),
    );
    // perm_input[2] = col_id
    builder.assert_zero(
        first_gate.clone() * (local.perm_input[2].clone().into() - local.col_id.clone().into()),
    );
    // perm_input[3..6] = key limbs
    builder.assert_zero(
        first_gate.clone() * (local.perm_input[3].clone().into() - local.key.limb0.clone().into()),
    );
    builder.assert_zero(
        first_gate.clone() * (local.perm_input[4].clone().into() - local.key.limb1.clone().into()),
    );
    builder.assert_zero(
        first_gate.clone() * (local.perm_input[5].clone().into() - local.key.limb2.clone().into()),
    );
    // perm_input[6..6+W] = new_val
    for i in 0..W {
        builder.assert_zero(
            first_gate.clone()
                * (local.perm_input[6 + i].clone().into() - local.new_val[i].clone().into()),
        );
    }
    // perm_input[6+W..16] = 0
    for i in (6 + W)..16 {
        builder.assert_zero(first_gate.clone() * local.perm_input[i].clone().into());
    }

    // ── Continuation composition (local part, gated by in_new AND NOT is_first_in_new) ──
    let cont_gate: AB::Expr = local.is_real.clone().into() * in_new * not_first_in_new;
    // perm_input[8..11] = key limbs
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[8].clone().into() - local.key.limb0.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[9].clone().into() - local.key.limb1.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[10].clone().into() - local.key.limb2.clone().into()),
    );
    // perm_input[11..11+W] = new_val
    for i in 0..W {
        builder.assert_zero(
            cont_gate.clone()
                * (local.perm_input[11 + i].clone().into() - local.new_val[i].clone().into()),
        );
    }
    // perm_input[11+W..16] = 0
    for i in (11 + W)..16 {
        builder.assert_zero(cont_gate.clone() * local.perm_input[i].clone().into());
    }

    // ── Continuation transition constraint ──
    // When the next row is in_new AND NOT is_first_in_new AND same segment:
    //   next.perm_input[0..8] = local.hash_acc[0..8]
    let trans_gate: AB::Expr = both_real
        * (AB::Expr::ONE - local.tc_changed.clone().into())
        * next.in_new.clone().into()
        * (AB::Expr::ONE - next.is_first_in_new.clone().into());
    for j in 0..8 {
        builder.when_transition().assert_zero(
            trans_gate.clone()
                * (next.perm_input[j].clone().into() - local.hash_acc[j].clone().into()),
        );
    }
}

// ── LogUp bus interactions ──────────────────────────────────────────────────

/// C3 MergeOldList bus receive.
///
/// Tuple: `(table_id, col_id, key_l0, key_l1, key_l2, old_val[0..W])`.
/// Multiplicity: `is_real · (is_old_only + is_both + is_delete)`.
///
/// Where: is_old_only = (1−s1)(1−s0), is_both = s1(1−s0), is_delete = s1·s0.
/// Sum = (1−s1)(1−s0) + s1(1−s0) + s1·s0 = (1−s0) + s1·s0 = 1 − s0 + s1·s0 = 1 − s0·(1−s1).
/// Simplification: mult = is_real · (1 − is_write_only) where is_write_only = (1−s1)·s0.
fn receive_merge_old_list<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    // is_old_only + is_both + is_delete = 1 − (1−s1)·s0
    let is_write_only: AB::Expr =
        (AB::Expr::ONE - local.s1.clone().into()) * local.s0.clone().into();
    let multiplicity: AB::Expr = local.is_real.clone().into() * (AB::Expr::ONE - is_write_only);

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.key.limb0.clone().into(),
        local.key.limb1.clone().into(),
        local.key.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.old_val[i].clone().into());
    }

    builder.receive(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::MergeOldList,
    });
}

/// C4 MergeWriteSet bus receive.
///
/// Tuple: `(table_id, col_id, key_l0, key_l1, key_l2, write_val[0..W], is_delete)`.
/// Multiplicity: `is_real · (is_write_only + is_both + is_delete)`.
///
/// Where: is_write_only = (1−s1)·s0, is_both = s1(1−s0), is_delete = s1·s0.
/// Sum = (1−s1)·s0 + s1(1−s0) + s1·s0 = s0 − s1·s0 + s1 = s0 + s1·(1−s0).
/// Simplification: mult = is_real · (s0 + s1 − s0·s1) = is_real · (1 − is_old_only).
fn receive_merge_write_set<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    // is_write_only + is_both + is_delete = 1 − (1−s1)(1−s0)
    let is_old_only: AB::Expr =
        (AB::Expr::ONE - local.s1.clone().into()) * (AB::Expr::ONE - local.s0.clone().into());
    let multiplicity: AB::Expr = local.is_real.clone().into() * (AB::Expr::ONE - is_old_only);

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.key.limb0.clone().into(),
        local.key.limb1.clone().into(),
        local.key.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.write_val[i].clone().into());
    }
    // is_delete = s1·s0 serves as val_is_null on receive side
    let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
    values.push(is_delete);

    builder.receive(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::MergeWriteSet,
    });
}

/// C6 CommitmentVerification bus send (NewList commitment).
///
/// Tuple: `(table_id, col_id, 1, 1, hash_acc[0..8])`.
/// Multiplicity: `is_real · is_last_segment`.
///
/// Merge only sends Com_new (comm_type=1) and always for touched columns (is_touched=1).
fn send_commitment_verification<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.is_last_segment.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        AB::Expr::ONE, // comm_type = 1 (Com_new)
        AB::Expr::ONE, // is_touched = 1 (Merge only exists for touched columns)
    ];
    for j in 0..8 {
        values.push(local.hash_acc[j].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::CommitmentVerification,
    });
}

/// C5 PoseidonPermutation bus send.
///
/// Tuple: `(perm_input[0..16], hash_acc[0..8])` — 24 elements.
/// Multiplicity: `is_real · in_new`.
///
/// Only rows with `in_new=1` contribute to the NewList hash chain.
/// Rows with `in_new=0` (delete) have zero multiplicity.
fn send_poseidon_permutation<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.in_new.clone().into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for j in 0..16 {
        values.push(local.perm_input[j].clone().into());
    }
    for j in 0..8 {
        values.push(local.hash_acc[j].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::PoseidonPermutation,
    });
}
