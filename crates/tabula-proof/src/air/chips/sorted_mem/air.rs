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
//! LogUp declarations: Memory bus receive (multiplicity = is_real * (1 - is_init)).
//! Deferred to M9 (LogUp wiring).

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::columns::borrow_cols;
use crate::air::gadgets::integer::{SHIFT_30_U32, expr_from_u32};
use crate::air::gadgets::{
    U64Limbs, constrain_is_real_prefix, constrain_is_zero, constrain_limb_halves,
    constrain_null_canon,
};
use crate::air::interaction::{AirInteraction, InteractionKind};

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
        constrain_is_real(builder, local, next);
        constrain_null_canon_all(builder, local);
        constrain_init_format(builder, local);
        constrain_same_key_detection(builder, local, next, both_real.clone());
        constrain_segment_first_init(builder, local, next, both_real.clone());
        constrain_first_of_segment(builder, local, next, both_real.clone());
        constrain_ordering(builder, local, next, both_real.clone());
        constrain_memory_transitions(builder, local, next, both_real.clone());
        // 9. Init-row uniqueness: implied by tau ordering (init has tau=0,
        //    next same-key row must have tau > 0 via StrictIneq). No extra constraint.
        constrain_write_set_extraction(builder, local, next, both_real);

        constrain_range_check_halves(builder, local);

        // ── LogUp buses ──
        receive_memory(builder, local);
        send_sorted_mem_meta(builder, local);
        send_ssmc_membership(builder, local);
        send_merge_write_set(builder, local);
        send_range_checks(builder, local);
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// Reconstruct a u64 from its 3 BabyBear limbs as an `AB::Expr`.
fn reconstruct_u64<AB: AirBuilder>(limbs: &U64Limbs<AB::Var>) -> AB::Expr {
    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();
    limbs.limb0.clone().into()
        + limbs.limb1.clone().into() * shift_30
        + limbs.limb2.clone().into() * shift_60
}

/// 1. Boolean constraints on 10 flag columns (is_real handled by is_real_prefix).
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
    builder.assert_bool(local.tc_changed.clone());
    builder.assert_bool(local.r_changed.clone());
    builder.assert_bool(local.is_first_of_segment.clone());
    builder.assert_bool(local.meta_is_empty_old.clone());
}

/// 2. `is_real` prefix: monotonic 1→0 transition.
fn constrain_is_real<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
) {
    constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
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
        .assert_zero(is_init.clone() * local.tau.limb0.clone().into());
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.tau.limb1.clone().into());
    builder
        .when(local.is_real.clone())
        .assert_zero(is_init.clone() * local.tau.limb2.clone().into());

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

/// 5-6. Same-key detection via IsZero gadgets + tc_changed / r_changed derivation.
fn constrain_same_key_detection<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
    let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();

    constrain_is_zero(builder, table_diff, &local.table_diff_iz);
    constrain_is_zero(builder, col_diff, &local.col_diff_iz);

    // r_diff: combined diff over 3 limbs.
    let r_combined_diff: AB::Expr =
        reconstruct_u64::<AB>(&next.r) - reconstruct_u64::<AB>(&local.r);
    constrain_is_zero(builder, r_combined_diff, &local.r_diff_iz);

    // tc_changed = 1 iff table or col changed from this row to next.
    // tc_changed = 1 - table_same * col_same
    let table_same: AB::Expr = local.table_diff_iz.is_zero.clone().into();
    let col_same: AB::Expr = local.col_diff_iz.is_zero.clone().into();
    let expected_tc_changed: AB::Expr = AB::Expr::ONE - table_same.clone() * col_same.clone();
    builder
        .when_transition()
        .assert_zero(both_real.clone() * (local.tc_changed.clone().into() - expected_tc_changed));

    // r_changed = 1 iff the full (t,c,r) key differs from the next row.
    // r_changed = 1 - tc_same * r_same
    let r_same: AB::Expr = local.r_diff_iz.is_zero.clone().into();
    let tc_same: AB::Expr = table_same * col_same;
    let expected_r_changed: AB::Expr = AB::Expr::ONE - tc_same * r_same;
    builder
        .when_transition()
        .assert_zero(both_real * (local.r_changed.clone().into() - expected_r_changed));
}

/// 5. Segment-first init: on (t,c) boundary, next row must be init.
///
/// Also: the very first real row must be an init row.
fn constrain_segment_first_init<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // tc_changed * both_real * (1 - next.is_init) = 0
    builder.when_transition().assert_zero(
        local.tc_changed.clone().into() * both_real * (AB::Expr::ONE - next.is_init.clone().into()),
    );

    // The very first real row must be an init row.
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(AB::Expr::ONE - local.is_init.clone().into());
}

/// First-of-segment derivation: `is_first_of_segment` marks the first row of each `(t,c)` group.
///
/// - First real row: always first-of-segment.
/// - Transition: `next.is_first_of_segment = local.tc_changed` (when both real).
fn constrain_first_of_segment<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // First real row must be first-of-segment.
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_zero(AB::Expr::ONE - local.is_first_of_segment.clone().into());

    // For real transitions: next.is_first_of_segment = local.tc_changed.
    builder.when_transition().assert_zero(
        both_real * (next.is_first_of_segment.clone().into() - local.tc_changed.clone().into()),
    );
}

/// Range-check half-decomposition constraints.
///
/// Each 30-bit limb of `r` and `tau` is decomposed into two 15-bit halves.
/// Constrains: `limb = lo + hi * 2^15` for all 4 decomposed limbs.
///
/// No `is_real` gating needed — on padding rows all values are zero,
/// so `0 = 0 + 0 * 2^15` is trivially satisfied.
fn constrain_range_check_halves<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    constrain_limb_halves(builder, local.r.limb0.clone().into(), &local.r_l0_halves);
    constrain_limb_halves(builder, local.r.limb1.clone().into(), &local.r_l1_halves);
    constrain_limb_halves(
        builder,
        local.tau.limb0.clone().into(),
        &local.tau_l0_halves,
    );
    constrain_limb_halves(
        builder,
        local.tau.limb1.clone().into(),
        &local.tau_l1_halves,
    );
}

/// C1 Memory bus receive: non-init rows receive from ExecutionChip.
///
/// Tuple: `(table_id, col_id, r[3], tau[3], is_write, val[W], val_is_null)`.
/// Multiplicity: `is_real · (1 − is_init)`.
fn receive_memory<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - local.is_init.clone().into());

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.r.limb0.clone().into(),
        local.r.limb1.clone().into(),
        local.r.limb2.clone().into(),
        local.tau.limb0.clone().into(),
        local.tau.limb1.clone().into(),
        local.tau.limb2.clone().into(),
        local.is_write.clone().into(),
    ];
    for i in 0..W {
        values.push(local.val[i].clone().into());
    }
    values.push(local.val_is_null.clone().into());

    builder.receive(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::Memory,
    });
}

/// SortedMemMeta bus send: one interaction per `(t,c)` segment.
///
/// Tuple: `(table_id, col_id, meta_is_empty_old)`.
/// Multiplicity: `is_real · is_first_of_segment`.
fn send_sorted_mem_meta<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.is_first_of_segment.clone().into();

    builder.send(AirInteraction {
        values: vec![
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            local.meta_is_empty_old.clone().into(),
        ],
        multiplicity,
        kind: InteractionKind::SortedMemMeta,
    });
}

/// C2 SsmcMembership bus send: init rows for non-empty columns with non-null values.
///
/// Tuple: `(table_id, col_id, r_l0, r_l1, r_l2, val[0..W])`.
/// Multiplicity: `is_real · is_init · (1 − val_is_null) · (1 − meta_is_empty_old)`.
fn send_ssmc_membership<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into()
        * local.is_init.clone().into()
        * (AB::Expr::ONE - local.val_is_null.clone().into())
        * (AB::Expr::ONE - local.meta_is_empty_old.clone().into());

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.r.limb0.clone().into(),
        local.r.limb1.clone().into(),
        local.r.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.val[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::SsmcMembership,
    });
}

/// C4 MergeWriteSet bus send: write-set extraction rows.
///
/// Tuple: `(table_id, col_id, r_l0, r_l1, r_l2, mem[0..W], mem_is_null)`.
/// Multiplicity: `is_real · is_last_for_key · has_written`.
fn send_merge_write_set<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into()
        * local.is_last_for_key.clone().into()
        * local.has_written.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.r.limb0.clone().into(),
        local.r.limb1.clone().into(),
        local.r.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.mem[i].clone().into());
    }
    values.push(local.mem_is_null.clone().into());

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::MergeWriteSet,
    });
}

/// 7. Ordering: shared StrictIneq for r (key change) or tau (same key).
fn constrain_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
    next: &GlobalSortedMemCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let cur_r = reconstruct_u64::<AB>(&local.r);
    let next_r = reconstruct_u64::<AB>(&next.r);
    let cur_tau = reconstruct_u64::<AB>(&local.tau);
    let next_tau = reconstruct_u64::<AB>(&next.tau);

    let shift_30: AB::Expr = expr_from_u32::<AB>(SHIFT_30_U32);
    let shift_60: AB::Expr = shift_30.clone() * shift_30.clone();
    let gap: AB::Expr = local.ordering.diff0.clone().into()
        + local.ordering.diff1.clone().into() * shift_30
        + local.ordering.diff2.clone().into() * shift_60;

    // r ordering: active only within same (t,c) when r changes.
    let r_ordering_active: AB::Expr =
        (AB::Expr::ONE - local.tc_changed.clone().into()) * local.r_changed.clone().into();
    builder.when_transition().assert_zero(
        both_real.clone() * r_ordering_active * (gap.clone() - (next_r - cur_r) + AB::Expr::ONE),
    );

    // tau ordering: active when same key (r_changed=0).
    builder.when_transition().assert_zero(
        both_real
            * (AB::Expr::ONE - local.r_changed.clone().into())
            * (gap - (next_tau - cur_tau) + AB::Expr::ONE),
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

    // Read: is_write=0 ⟹ val = mem (reads return the current memory value).
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

    // For non-init same-key continuation (next.is_init=0 and r_changed=0):
    // next.has_written = local.has_written + (1 - local.has_written) * next.is_write
    //                  = local.has_written + next.is_write - local.has_written * next.is_write
    let same_key: AB::Expr = AB::Expr::ONE - local.r_changed.clone().into();
    let next_not_init: AB::Expr = AB::Expr::ONE - next.is_init.clone().into();
    let expected_next_hw: AB::Expr = local.has_written.clone().into()
        + next.is_write.clone().into()
        - local.has_written.clone().into() * next.is_write.clone().into();
    builder.when_transition().assert_zero(
        both_real * same_key * next_not_init * (next.has_written.clone().into() - expected_next_hw),
    );
}

/// C8 RangeCheck bus sends: half-decomposed limbs of `r` and `tau`.
///
/// Sends 10 values per real row:
/// - `r.limb0`: lo, hi (2 sends)
/// - `r.limb1`: lo, hi (2 sends)
/// - `r.limb2`: direct (1 send, 4-bit value fits in [0, 2^16))
/// - `tau.limb0`: lo, hi (2 sends)
/// - `tau.limb1`: lo, hi (2 sends)
/// - `tau.limb2`: direct (1 send)
///
/// All with multiplicity `is_real`.
fn send_range_checks<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSortedMemCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into();

    // Helper: send a single value on the RangeCheck bus.
    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            kind: InteractionKind::RangeCheck,
        });
    };

    // r limbs
    send_rc(local.r_l0_halves.lo.clone().into());
    send_rc(local.r_l0_halves.hi.clone().into());
    send_rc(local.r_l1_halves.lo.clone().into());
    send_rc(local.r_l1_halves.hi.clone().into());
    send_rc(local.r.limb2.clone().into());

    // tau limbs
    send_rc(local.tau_l0_halves.lo.clone().into());
    send_rc(local.tau_l0_halves.hi.clone().into());
    send_rc(local.tau_l1_halves.lo.clone().into());
    send_rc(local.tau_l1_halves.hi.clone().into());
    send_rc(local.tau.limb2.clone().into());
}
