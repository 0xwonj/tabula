//! InterTxOrderChip — AIR constraints for inter-transaction access ordering.
//!
//! Constraint groups:
//! 1. Boolean fields
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Init first: new key → must be init row
//! 4. Init shape: init → no read/write, output=input
//! 5. Access minimum: non-init → has_read OR has_write
//! 6. Read consistency: same_key read → input = prev.output
//! 7. Output derivation: no write → output = input
//! 8. Key ordering: same_tc, different key → strict inequality
//! 9. Tx ordering: same_key → tx_diff = next.tx_index - tx_index - 1
//! 10. Segment lex: different tc → lex ordering direction
//! 11. is_last_for_key: ↔ next row has different key
//! 12. has_ever_written: monotone within key; init→0; has_write→1
//! 13. Range checks: key halves, ordering halves, lex diffs, tx_diff
//!
//! LogUp buses:
//! - C10 ReadAccess receive
//! - C11 WriteAccess receive
//! - C13 BaseStateEntry send (init rows)
//! - C14 CoalescedWrite send (last-for-key with write)
//! - C8 RangeCheck sends

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{
    BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, ReadAccessAirBuilder, WriteAccessAirBuilder,
};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{
    constrain_is_real_prefix, constrain_is_zero, constrain_key_halves, constrain_lex_direction,
    constrain_ordering_halves, constrain_same_key_detection, constrain_strict_ineq,
    send_key_range_checks, send_lex_range_checks, send_ordering_range_checks,
};

use super::columns::{InterTxOrderCols, inter_tx_order_width};

/// The InterTxOrder AIR chip, generic over value width.
#[derive(Debug)]
pub struct InterTxOrderChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for InterTxOrderChip<W> {
    fn width(&self) -> usize {
        inter_tx_order_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for InterTxOrderChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &InterTxOrderCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &InterTxOrderCols<AB::Var, W> = borrow_cols(&next_row);

        let is_real: AB::Expr = local.is_real.clone().into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

        // Derive same_key (not a column — computed from tc + key limbs)
        let same_tc: AB::Expr = AB::Expr::ONE - local.same_tc.tc_changed.clone().into();
        let limb0_same: AB::Expr = local.r_limb0_iz.is_zero.clone().into();
        let limb1_same: AB::Expr = local.r_limb1_iz.is_zero.clone().into();
        let limb2_same: AB::Expr = local.r_limb2_iz.is_zero.clone().into();
        let same_key: AB::Expr =
            same_tc.clone() * limb0_same.clone() * limb1_same.clone() * limb2_same.clone();

        // ── 1. Boolean constraints ──
        constrain_booleans(builder, local);

        // ── 2. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // ── 3. Init first: new key → must be init ──
        constrain_init_first(builder, local, next, both_real.clone(), same_key.clone());

        // ── 4. Init shape ──
        constrain_init_shape(builder, local, is_real.clone());

        // ── 5. Access minimum ──
        constrain_access_minimum(builder, local, is_real.clone());

        // ── 6. Read consistency ──
        constrain_read_consistency::<AB, W>(
            builder,
            local,
            next,
            both_real.clone(),
            same_key.clone(),
        );

        // ── 7. Output derivation ──
        constrain_output_derivation(builder, local, is_real.clone());

        // ── 8. Key ordering ──
        constrain_key_ordering(
            builder,
            local,
            next,
            both_real.clone(),
            same_tc.clone(),
            same_key.clone(),
        );

        // ── 9. Tx ordering ──
        constrain_tx_ordering(builder, local, next, both_real.clone(), same_key.clone());

        // ── 10. Segment lex ordering ──
        constrain_segment_lex(builder, local, next, both_real.clone());

        // ── 11. is_last_for_key ──
        constrain_is_last_for_key(
            builder,
            local,
            next,
            is_real.clone(),
            both_real.clone(),
            same_key.clone(),
        );

        // ── 12. has_ever_written ──
        constrain_has_ever_written(
            builder,
            local,
            next,
            is_real.clone(),
            both_real.clone(),
            same_key.clone(),
        );

        // ── 13. Range checks ──
        // Key halves
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // Same-key detection gadgets (tc + key limb diffs)
        {
            let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
            let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();
            constrain_same_key_detection(
                builder,
                &local.same_tc,
                table_diff,
                col_diff,
                both_real.clone(),
            );
        }

        // Key limb IsZero gadgets
        // NOTE: constrain_is_zero is unconditionally applied (no is_real guard).
        // The trace generator MUST populate these for ALL rows including padding.
        {
            let diff0: AB::Expr =
                next.key.limbs.limb0.clone().into() - local.key.limbs.limb0.clone().into();
            constrain_is_zero(builder, diff0, &local.r_limb0_iz);

            let diff1: AB::Expr =
                next.key.limbs.limb1.clone().into() - local.key.limbs.limb1.clone().into();
            constrain_is_zero(builder, diff1, &local.r_limb1_iz);

            let diff2: AB::Expr =
                next.key.limbs.limb2.clone().into() - local.key.limbs.limb2.clone().into();
            constrain_is_zero(builder, diff2, &local.r_limb2_iz);
        }

        // C8 RangeCheck sends
        send_key_range_checks(builder, &local.key, is_real.clone());
        {
            // Ordering range checks: active when same segment, different key
            let diff_key: AB::Expr = AB::Expr::ONE - same_key.clone();
            send_ordering_range_checks(
                builder,
                &local.key_ordering,
                both_real.clone() * same_tc.clone() * diff_key,
            );
        }
        {
            let tc_changed: AB::Expr = local.same_tc.tc_changed.clone().into();
            send_lex_range_checks(builder, &local.lex_dir, both_real.clone() * tc_changed);
        }
        // tx_diff range check (u16): between consecutive access rows
        {
            let not_init_local: AB::Expr = AB::Expr::ONE - local.is_init.clone().into();
            let not_init_next: AB::Expr = AB::Expr::ONE - next.is_init.clone().into();
            builder.send(crate::air::interaction::AirInteraction {
                values: vec![local.tx_diff.clone().into()],
                multiplicity: both_real.clone() * same_key.clone() * not_init_local * not_init_next,
                kind: crate::air::interaction::InteractionKind::RangeCheck,
            });
        }

        // ── LogUp buses ──

        // C10 ReadAccess receive: non-init rows with has_read
        builder.receive_read_access(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.input_val,
            local.input_is_null.clone().into(),
            is_real.clone()
                * local.has_read.clone().into()
                * (AB::Expr::ONE - local.is_init.clone().into()),
        );

        // C11 WriteAccess receive: non-init rows with has_write
        builder.receive_write_access(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.output_val,
            local.output_is_null.clone().into(),
            is_real.clone()
                * local.has_write.clone().into()
                * (AB::Expr::ONE - local.is_init.clone().into()),
        );

        // C13 BaseStateEntry send: init rows
        builder.send_base_state_entry(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.input_val,
            local.input_is_null.clone().into(),
            is_real.clone() * local.is_init.clone().into(),
        );

        // C14 CoalescedWrite send: last-for-key rows that had a write
        builder.send_coalesced_write(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.output_val,
            local.output_is_null.clone().into(),
            is_real * local.is_last_for_key.clone().into() * local.has_ever_written.clone().into(),
        );
    }
}

// ── Constraint helpers ───────────────────────────────────────────────────────

/// 1. Boolean constraints on all flag columns.
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_init.clone());
    builder.assert_bool(local.has_read.clone());
    builder.assert_bool(local.has_write.clone());
    builder.assert_bool(local.is_last_for_key.clone());
    builder.assert_bool(local.has_ever_written.clone());
    builder.assert_bool(local.input_is_null.clone());
    builder.assert_bool(local.output_is_null.clone());
}

/// 3. Init first: when key changes (not same_key), next row must be init.
///    First real row must also be init.
fn constrain_init_first<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    // When both rows are real and key differs, next must be init
    let diff_key: AB::Expr = AB::Expr::ONE - same_key;
    builder
        .when_transition()
        .assert_zero(both_real * diff_key * (AB::Expr::ONE - next.is_init.clone().into()));
    // First row must be init (if real)
    builder
        .when_first_row()
        .assert_zero(local.is_real.clone().into() * (AB::Expr::ONE - local.is_init.clone().into()));
}

/// 4. Init shape: init → has_read=0, has_write=0, output=input.
fn constrain_init_shape<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.is_init.clone().into();
    builder.assert_zero(gate.clone() * local.has_read.clone().into());
    builder.assert_zero(gate.clone() * local.has_write.clone().into());
    for i in 0..W {
        let diff: AB::Expr = local.output_val[i].clone().into() - local.input_val[i].clone().into();
        builder.assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr =
        local.output_is_null.clone().into() - local.input_is_null.clone().into();
    builder.assert_zero(gate * null_diff);
}

/// 5. Access minimum: non-init real rows must have at least has_read or has_write.
fn constrain_access_minimum<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_init: AB::Expr = AB::Expr::ONE - local.is_init.clone().into();
    let gate: AB::Expr = is_real * not_init;
    // has_read + has_write - has_read * has_write ≥ 1
    // Equivalently: (1 - has_read) * (1 - has_write) = 0
    let no_read: AB::Expr = AB::Expr::ONE - local.has_read.clone().into();
    let no_write: AB::Expr = AB::Expr::ONE - local.has_write.clone().into();
    builder.assert_zero(gate * no_read * no_write);
}

/// 6. Read consistency: when same_key and next row reads, next.input = local.output.
fn constrain_read_consistency<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let gate: AB::Expr = both_real * same_key * next.has_read.clone().into();
    for i in 0..W {
        let diff: AB::Expr = next.input_val[i].clone().into() - local.output_val[i].clone().into();
        builder.when_transition().assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr =
        next.input_is_null.clone().into() - local.output_is_null.clone().into();
    builder.when_transition().assert_zero(gate * null_diff);
}

/// 7. Output derivation: no write → output = input.
fn constrain_output_derivation<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_init: AB::Expr = AB::Expr::ONE - local.is_init.clone().into();
    let no_write: AB::Expr = AB::Expr::ONE - local.has_write.clone().into();
    let gate: AB::Expr = is_real * not_init * no_write;
    for i in 0..W {
        let diff: AB::Expr = local.output_val[i].clone().into() - local.input_val[i].clone().into();
        builder.assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr =
        local.output_is_null.clone().into() - local.input_is_null.clone().into();
    builder.assert_zero(gate * null_diff);
}

/// 8. Key ordering: same_tc, different key → strict inequality.
fn constrain_key_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    both_real: AB::Expr,
    same_tc: AB::Expr,
    same_key: AB::Expr,
) {
    let diff_key: AB::Expr = AB::Expr::ONE - same_key;
    let gate: AB::Expr = both_real * same_tc * diff_key;
    let mut when_transition = builder.when_transition();
    let mut when_gate = when_transition.when(gate);
    constrain_strict_ineq(
        &mut when_gate,
        &local.key.limbs,
        &next.key.limbs,
        &local.key_ordering.ineq,
    );
}

/// 9. Tx ordering: between consecutive access rows for the same key,
///    tx_diff = next.tx_index - local.tx_index - 1 (non-negative, range-checked).
fn constrain_tx_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    // Only between two non-init rows (init → access transition has no tx ordering)
    let not_init_local: AB::Expr = AB::Expr::ONE - local.is_init.clone().into();
    let not_init_next: AB::Expr = AB::Expr::ONE - next.is_init.clone().into();
    let gate: AB::Expr = both_real * same_key * not_init_local * not_init_next;
    let expected: AB::Expr =
        next.tx_index.clone().into() - local.tx_index.clone().into() - AB::Expr::ONE;
    builder
        .when_transition()
        .assert_zero(gate * (local.tx_diff.clone().into() - expected));
}

/// 10. Segment lex ordering at (t,c) boundaries.
fn constrain_segment_lex<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let gate: AB::Expr = both_real * local.same_tc.tc_changed.clone().into();
    constrain_lex_direction(
        builder,
        &local.lex_dir,
        next.table_id.clone().into(),
        local.table_id.clone().into(),
        next.col_id.clone().into(),
        local.col_id.clone().into(),
        gate,
    );
}

/// 11. is_last_for_key: true iff next row has different key (or is padding).
fn constrain_is_last_for_key<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let not_same_key: AB::Expr = AB::Expr::ONE - same_key;

    // When both rows real: is_last_for_key = !same_key
    builder
        .when_transition()
        .assert_zero(both_real * (local.is_last_for_key.clone().into() - not_same_key));

    // When real→padding: must be last for key
    let real_to_padding: AB::Expr = is_real * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last_for_key.clone().into()));
}

/// 12. has_ever_written: determined by init→0, then OR propagation within key.
///
/// Two constraints fully determine the flag:
/// - Init rows: `has_ever_written = 0`
/// - Same-key transitions: `next.hew = local.hew OR next.has_write`
///
/// The OR formula subsumes both monotonicity (`hew=1` stays 1) and
/// the `has_write → hew=1` requirement.
fn constrain_has_ever_written<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &InterTxOrderCols<AB::Var, W>,
    next: &InterTxOrderCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    // Init → has_ever_written = 0
    builder.assert_zero(
        is_real * local.is_init.clone().into() * local.has_ever_written.clone().into(),
    );

    // Same-key transition: next.hew = local.hew OR next.has_write
    // = local.hew + next.hw - local.hew * next.hw
    let local_hew: AB::Expr = local.has_ever_written.clone().into();
    let next_hw: AB::Expr = next.has_write.clone().into();
    let expected: AB::Expr = local_hew.clone() + next_hw.clone() - local_hew * next_hw;
    builder
        .when_transition()
        .assert_zero(both_real * same_key * (next.has_ever_written.clone().into() - expected));
}
