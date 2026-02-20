//! GlobalSortedMemChip — AIR constraints for the sorted memory table.
//!
//! The GlobalSortedMem table proves memory consistency across all transactions
//! in a batch. Rows are sorted by `(table_id, col_id, r, tau)`.
//!
//! Constraints (proof-spec §8):
//! 1. Boolean fields (9): is_real, is_init, is_write, val_is_null, mem_is_null,
//!    is_last_for_key, has_written, tc_changed, r_changed
//! 2. `is_real` prefix: monotonic 1→0 transition
//! 3. Null canonicality: val_is_null=1 ⟹ val[i]=0; mem_is_null=1 ⟹ mem[i]=0
//! 4. Init format: is_init=1 ⟹ tau=0 ∧ is_write=0 ∧ mem=val ∧ mem_is_null=val_is_null
//! 5. Segment-first init: on (t,c) boundary, next row must be init
//! 6. Same-key detection: tc_changed / r_changed via IsZero gadgets
//! 7. Ordering: shared StrictIneq for r (key change) or tau (same key)
//! 8. Memory transitions: read (val=mem) / write (next_mem=val)
//! 9. Init-row uniqueness: after init with same key, tau > 0
//! 10. Write-set extraction: is_last_for_key / has_written propagation
//!
//! LogUp buses:
//! - C1 Memory receive: non-init rows
//! - C7 SortedMemMeta send: one per (t,c) segment
//! - C2 SsmcMembership send: init rows for non-empty columns
//! - C4 MergeWriteSet send: write-set extraction rows

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{
    MemoryAirBuilder, MergeAirBuilder, SortedMemMetaAirBuilder, SsmcMembershipAirBuilder,
};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::integer::{SHIFT_30_U32, expr_from_u32};
use crate::air::gadgets::{
    constrain_is_real_prefix, constrain_is_zero, constrain_key_halves, constrain_lex_direction,
    constrain_null_canon, constrain_ordering_halves, constrain_same_key_detection,
    send_key_range_checks, send_lex_range_checks, send_ordering_range_checks,
};

use super::columns::{GlobalSortedMemCols, sorted_mem_width};

/// The GlobalSortedMem AIR chip, generic over value width.
#[derive(Debug)]
pub struct GlobalSortedMemChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for GlobalSortedMemChip<W> {
    fn width(&self) -> usize {
        sorted_mem_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for GlobalSortedMemChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &GlobalSortedMemCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &GlobalSortedMemCols<AB::Var, W> = borrow_cols(&next_row);

        let both_real: AB::Expr = local.is_real.clone().into() * next.is_real.clone().into();

        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
        constrain_null_canon_all(builder, local);
        constrain_init_format(builder, local);

        // ── Same-key detection ──
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
        constrain_r_changed(builder, local, next, both_real.clone());

        constrain_segment_first_init(builder, local, next, both_real.clone());
        constrain_first_of_segment(builder, local, next, both_real.clone());
        constrain_meta_constancy(builder, local, next, both_real.clone());
        constrain_ordering(builder, local, next, both_real.clone());
        constrain_memory_transitions(builder, local, next, both_real.clone());
        // 9. Init-row uniqueness: implied by tau ordering (init has tau=0,
        //    next same-key row must have tau > 0 via StrictIneq). No extra constraint.
        constrain_write_set_extraction(builder, local, next, both_real.clone());

        // ── Range check half-decomposition ──
        constrain_key_halves(builder, &local.r);
        constrain_key_halves(builder, &local.tau);
        constrain_ordering_halves(builder, &local.ordering);

        // ── Lex ordering direction ──
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

        // C1 Memory receive
        builder.receive_memory_access(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.r.limbs,
            &local.tau.limbs,
            local.is_write.clone().into(),
            &local.val,
            local.val_is_null.clone().into(),
            is_real.clone() * (AB::Expr::ONE - local.is_init.clone().into()),
        );

        // C7 SortedMemMeta send
        builder.send_sorted_mem_meta(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            local.meta_is_empty_old.clone().into(),
            is_real.clone() * local.is_first_of_segment.clone().into(),
        );

        // C2 SsmcMembership send
        builder.send_ssmc_membership(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.r.limbs,
            &local.val,
            is_real.clone()
                * local.is_init.clone().into()
                * (AB::Expr::ONE - local.val_is_null.clone().into())
                * (AB::Expr::ONE - local.meta_is_empty_old.clone().into()),
        );

        // C4 MergeWriteSet send
        builder.send_merge_write_set(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.r.limbs,
            &local.mem,
            local.mem_is_null.clone().into(),
            is_real.clone()
                * local.is_last_for_key.clone().into()
                * local.has_written.clone().into(),
        );

        // C8 RangeCheck sends
        send_key_range_checks(builder, &local.r, is_real.clone());
        send_key_range_checks(builder, &local.tau, is_real.clone());
        send_ordering_range_checks(builder, &local.ordering, is_real.clone());
        {
            let tc: AB::Expr = local.segment.tc_changed.clone().into();
            send_lex_range_checks(builder, &local.lex, is_real * tc);
        }
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// 1. Boolean constraints on flag columns (is_real handled by is_real_prefix).
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_init.clone());
    builder.assert_bool(local.is_write.clone());
    builder.assert_bool(local.val_is_null.clone());
    builder.assert_bool(local.mem_is_null.clone());
    builder.assert_bool(local.is_last_for_key.clone());
    builder.assert_bool(local.has_written.clone());
    builder.assert_bool(local.r_changed.clone());
    builder.assert_bool(local.is_first_of_segment.clone());
    builder.assert_bool(local.meta_is_empty_old.clone());
}

/// 3. Null canonicality: val_is_null=1 ⟹ val[i]=0; mem_is_null=1 ⟹ mem[i]=0.
fn constrain_null_canon_all<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    constrain_null_canon(builder, local.val_is_null.clone().into(), &local.val);
    constrain_null_canon(builder, local.mem_is_null.clone().into(), &local.mem);
}

/// 4. Init format: is_init=1 ⟹ tau=0, is_write=0, mem=val, mem_is_null=val_is_null.
fn constrain_init_format<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let is_init: AB::Expr = local.is_init.clone().into();

    // tau must be zero: each limb = 0
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.tau.limbs.limb0.clone().into());
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.tau.limbs.limb1.clone().into());
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.tau.limbs.limb2.clone().into());

    // is_write must be 0
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.is_write.clone().into());

    // mem = val (init seeds memory from base state)
    for i in 0..W {
        builder.when(local.is_real.clone()).assert_zero(
            is_init.clone() * (local.mem[i].clone().into() - local.val[i].clone().into()),
        );
    }
    builder.when(local.is_real.clone()).assert_zero(
        is_init * (local.mem_is_null.clone().into() - local.val_is_null.clone().into()),
    );
}

/// r_changed derivation: uses tc_changed from segment + per-limb r IsZero.
///
/// r_changed = 1 iff the full (t,c,r) key differs from the next row.
/// r_changed = 1 - tc_same * r_same, where r_same = all 3 limb IsZero flags.
fn constrain_r_changed<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // Per-limb IsZero for row key diff.
    let r_limb0_diff: AB::Expr =
        next.r.limbs.limb0.clone().into() - local.r.limbs.limb0.clone().into();
    let r_limb1_diff: AB::Expr =
        next.r.limbs.limb1.clone().into() - local.r.limbs.limb1.clone().into();
    let r_limb2_diff: AB::Expr =
        next.r.limbs.limb2.clone().into() - local.r.limbs.limb2.clone().into();
    constrain_is_zero(builder, r_limb0_diff, &local.r_limb0_diff_iz);
    constrain_is_zero(builder, r_limb1_diff, &local.r_limb1_diff_iz);
    constrain_is_zero(builder, r_limb2_diff, &local.r_limb2_diff_iz);

    let table_same: AB::Expr = local.segment.table_diff_iz.is_zero.clone().into();
    let col_same: AB::Expr = local.segment.col_diff_iz.is_zero.clone().into();
    let r_same: AB::Expr = local.r_limb0_diff_iz.is_zero.clone().into()
        * local.r_limb1_diff_iz.is_zero.clone().into()
        * local.r_limb2_diff_iz.is_zero.clone().into();
    let tc_same: AB::Expr = table_same * col_same;
    let expected_r_changed: AB::Expr = AB::Expr::ONE - tc_same * r_same;
    builder
        .when_transition()
        .assert_zero(both_real * (local.r_changed.clone().into() - expected_r_changed));
}

/// 5. Segment-first init: on (t,c) boundary, next row must be init.
fn constrain_segment_first_init<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    builder.when_transition().assert_zero(
        local.segment.tc_changed.clone().into()
            * both_real
            * (AB::Expr::ONE - next.is_init.clone().into()),
    );

    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(AB::Expr::ONE - local.is_init.clone().into());
}

/// First-of-segment derivation.
fn constrain_first_of_segment<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(AB::Expr::ONE - local.is_first_of_segment.clone().into());

    builder.when_transition().assert_zero(
        both_real
            * (next.is_first_of_segment.clone().into() - local.segment.tc_changed.clone().into()),
    );
}

/// 7. Ordering: shared StrictIneq for r (key change) or tau (same key).
///
/// Uses borrow-chain per-limb equations. The same diff/borrow columns are
/// shared between r ordering and tau ordering, gated by `r_changed`.
fn constrain_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let ineq = &local.ordering.ineq;

    // Borrow booleans (constrained always when both_real, regardless of which ordering)
    let same_tc: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    builder
        .when_transition()
        .when(both_real.clone())
        .when(same_tc.clone())
        .assert_bool(ineq.borrow0.clone());
    builder
        .when_transition()
        .when(both_real.clone())
        .when(same_tc.clone())
        .assert_bool(ineq.borrow1.clone());

    // r ordering: active within same (t,c) when r changes.
    let gate_r: AB::Expr =
        both_real.clone() * same_tc.clone() * local.r_changed.clone().into();

    // Limb 0: diff0 = next_r.l0 - cur_r.l0 - 1 + borrow0 * 2^30
    builder.when_transition().assert_zero(
        gate_r.clone()
            * (ineq.diff0.clone().into()
                - (next.r.limbs.limb0.clone().into()
                    - local.r.limbs.limb0.clone().into()
                    - AB::Expr::ONE
                    + ineq.borrow0.clone().into() * shift_30.clone())),
    );
    // Limb 1: diff1 = next_r.l1 - cur_r.l1 - borrow0 + borrow1 * 2^30
    builder.when_transition().assert_zero(
        gate_r.clone()
            * (ineq.diff1.clone().into()
                - (next.r.limbs.limb1.clone().into()
                    - local.r.limbs.limb1.clone().into()
                    - ineq.borrow0.clone().into()
                    + ineq.borrow1.clone().into() * shift_30.clone())),
    );
    // Limb 2: diff2 = next_r.l2 - cur_r.l2 - borrow1
    builder.when_transition().assert_zero(
        gate_r
            * (ineq.diff2.clone().into()
                - (next.r.limbs.limb2.clone().into()
                    - local.r.limbs.limb2.clone().into()
                    - ineq.borrow1.clone().into())),
    );

    // tau ordering: active when same key (r_changed=0, within same (t,c)).
    let gate_tau: AB::Expr =
        both_real * same_tc * (AB::Expr::ONE - local.r_changed.clone().into());

    builder.when_transition().assert_zero(
        gate_tau.clone()
            * (ineq.diff0.clone().into()
                - (next.tau.limbs.limb0.clone().into()
                    - local.tau.limbs.limb0.clone().into()
                    - AB::Expr::ONE
                    + ineq.borrow0.clone().into() * shift_30.clone())),
    );
    builder.when_transition().assert_zero(
        gate_tau.clone()
            * (ineq.diff1.clone().into()
                - (next.tau.limbs.limb1.clone().into()
                    - local.tau.limbs.limb1.clone().into()
                    - ineq.borrow0.clone().into()
                    + ineq.borrow1.clone().into() * shift_30)),
    );
    builder.when_transition().assert_zero(
        gate_tau
            * (ineq.diff2.clone().into()
                - (next.tau.limbs.limb2.clone().into()
                    - local.tau.limbs.limb2.clone().into()
                    - ineq.borrow1.clone().into())),
    );
}

/// 8. Memory transitions: read (val=mem), write (next_mem=val), carry (next_mem=mem).
fn constrain_memory_transitions<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_key: AB::Expr = AB::Expr::ONE - local.r_changed.clone().into();

    // Read: is_write=0 ⟹ val = mem
    let is_read: AB::Expr = AB::Expr::ONE - local.is_write.clone().into();
    for i in 0..W {
        builder.when(local.is_real.clone()).assert_zero(
            is_read.clone() * (local.val[i].clone().into() - local.mem[i].clone().into()),
        );
    }
    builder.when(local.is_real.clone()).assert_zero(
        is_read * (local.val_is_null.clone().into() - local.mem_is_null.clone().into()),
    );

    // Write transition: next.is_write=1 ⟹ next.mem = next.val
    for i in 0..W {
        builder.when_transition().assert_zero(
            both_real.clone()
                * same_key.clone()
                * next.is_write.clone().into()
                * (next.mem[i].clone().into() - next.val[i].clone().into()),
        );
    }
    builder.when_transition().assert_zero(
        both_real.clone()
            * same_key.clone()
            * next.is_write.clone().into()
            * (next.mem_is_null.clone().into() - next.val_is_null.clone().into()),
    );

    // Carry: next is a read (not write) ⟹ next.mem = local.mem
    let next_is_read: AB::Expr = AB::Expr::ONE - next.is_write.clone().into();
    for i in 0..W {
        builder.when_transition().assert_zero(
            both_real.clone()
                * same_key.clone()
                * next_is_read.clone()
                * (next.mem[i].clone().into() - local.mem[i].clone().into()),
        );
    }
    builder.when_transition().assert_zero(
        both_real
            * same_key
            * next_is_read
            * (next.mem_is_null.clone().into() - local.mem_is_null.clone().into()),
    );
}

/// meta_is_empty_old must be constant within a (t,c) segment.
fn constrain_meta_constancy<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    builder.when_transition().assert_zero(
        both_real
            * same_segment
            * (next.meta_is_empty_old.clone().into() - local.meta_is_empty_old.clone().into()),
    );
}

/// 10. Write-set extraction: is_last_for_key / has_written propagation.
fn constrain_write_set_extraction<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // On transition rows: is_last_for_key = r_changed
    builder.when_transition().assert_zero(
        both_real.clone() * (local.is_last_for_key.clone().into() - local.r_changed.clone().into()),
    );

    // On the last real row (real followed by padding): is_last_for_key must be 1.
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last_for_key.clone().into()));

    // has_written: on init rows, has_written = 0 (since is_write=0 on init).
    builder
        .when(local.is_real.clone())
        .assert_zero(local.is_init.clone().into() * local.has_written.clone().into());

    // For non-init same-key continuation:
    // next.has_written = local.has_written + (1 - local.has_written) * next.is_write
    let same_key: AB::Expr = AB::Expr::ONE - local.r_changed.clone().into();
    let next_not_init: AB::Expr = AB::Expr::ONE - next.is_init.clone().into();
    let expected_next_hw: AB::Expr = local.has_written.clone().into()
        + next.is_write.clone().into()
        - local.has_written.clone().into() * next.is_write.clone().into();
    builder.when_transition().assert_zero(
        both_real * same_key * next_not_init * (next.has_written.clone().into() - expected_next_hw),
    );
}
