//! GlobalSSMCChip — AIR constraints for the SSMC commitment table.
//!
//! The GlobalSSMC table proves sorted-set membership commitments for each
//! SSMC-committed column. Rows are sorted by `(table_id, col_id, key)`.
//!
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (4): is_first, is_last, mult_witness, segment_is_touched
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Key sorted uniqueness: within same segment, key_next > key
//! 4. Boundary flags: is_first/is_last consistency with tc_changed
//! 5. Segment lex ordering: (t,c) strictly increases across segments
//! 6. segment_is_touched constancy within segment
//!
//! LogUp buses:
//! - C2 SsmcMembership receive: `(t, c, key[3], value[W])`, mult = `is_real · mult_witness`
//! - C3 MergeOldList send: `(t, c, key[3], value[W])`, mult = `is_real · segment_is_touched`

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::bus::{
    CommitmentAirBuilder, MergeAirBuilder, PoseidonAirBuilder, SsmcMembershipAirBuilder,
};
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{
    constrain_hash_chain_input, constrain_hash_chain_transition, constrain_is_real_prefix,
    constrain_key_halves, constrain_lex_direction, constrain_ordering_halves,
    constrain_same_key_detection, constrain_strict_ineq, send_key_range_checks,
    send_lex_range_checks, send_ordering_range_checks,
};

use super::columns::{GlobalSsmcCols, ssmc_width};

/// The GlobalSSMC AIR chip, generic over value width.
#[derive(Debug)]
pub struct GlobalSsmcChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for GlobalSsmcChip<W> {
    fn width(&self) -> usize {
        ssmc_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for GlobalSsmcChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &GlobalSsmcCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &GlobalSsmcCols<AB::Var, W> = borrow_cols(&next_row);

        let both_real: AB::Expr = local.is_real.clone().into() * next.is_real.clone().into();

        // ── 1. Boolean constraints ──
        builder.assert_bool(local.is_first.clone());
        builder.assert_bool(local.is_last.clone());
        builder.assert_bool(local.mult_witness.clone());
        builder.assert_bool(local.segment_is_touched.clone());

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

        // ── 5. Boundary flags ──
        constrain_boundary_flags(builder, local, next, both_real.clone());

        // ── 6. segment_is_touched constancy ──
        {
            let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
            let diff: AB::Expr =
                next.segment_is_touched.clone().into() - local.segment_is_touched.clone().into();
            builder
                .when_transition()
                .assert_zero(both_real.clone() * same_segment * diff);
        }

        // ── 7. Hash chain input composition ──
        {
            let is_real: AB::Expr = local.is_real.clone().into();
            let is_first: AB::Expr = local.is_first.clone().into();
            let first_gate = is_real.clone() * is_first;
            let not_first: AB::Expr = AB::Expr::ONE - local.is_first.clone().into();
            let cont_gate = is_real * not_first;
            constrain_hash_chain_input::<AB, W>(
                builder,
                &local.hash_chain,
                &local.key.limbs,
                &local.value,
                local.table_id.clone(),
                local.col_id.clone(),
                first_gate,
                cont_gate,
            );
            let trans_gate: AB::Expr =
                both_real.clone() * (AB::Expr::ONE - next.is_first.clone().into());
            constrain_hash_chain_transition(
                builder,
                &next.hash_chain.perm_input,
                &local.hash_acc,
                trans_gate,
            );
        }

        // ── 8. Range check half-decomposition ──
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // ── 9. Lex ordering direction ──
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

        // C2 SsmcMembership receive
        builder.receive_ssmc_membership(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.value,
            is_real.clone() * local.mult_witness.clone().into(),
        );

        // C3 MergeOldList send
        builder.send_merge_old_list(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.value,
            is_real.clone() * local.segment_is_touched.clone().into(),
        );

        // C6 CommitmentVerification send (OldList commitment at segment end)
        builder.send_commitment(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            AB::Expr::ZERO, // comm_type = 0 (Com_old)
            local.segment_is_touched.clone().into(),
            &local.hash_acc,
            is_real.clone() * local.is_last.clone().into(),
        );

        // C5 PoseidonPermutation send
        builder.send_poseidon_perm(
            &local.hash_chain.perm_input,
            &local.hash_acc,
            is_real.clone(),
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

/// 5. Boundary flag constraints.
fn constrain_boundary_flags<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    // First real row must have is_first = 1.
    builder
        .when_first_row()
        .when(local.is_real.clone())
        .assert_one(local.is_first.clone());

    // When tc_changed (both real): current row is last, next row is first.
    builder.when_transition().assert_zero(
        both_real.clone()
            * local.segment.tc_changed.clone().into()
            * (AB::Expr::ONE - local.is_last.clone().into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * local.segment.tc_changed.clone().into()
            * (AB::Expr::ONE - next.is_first.clone().into()),
    );

    // When NOT tc_changed (both real): current row is not last, next row is not first.
    let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real.clone() * same_segment.clone() * local.is_last.clone().into());
    builder
        .when_transition()
        .assert_zero(both_real.clone() * same_segment * next.is_first.clone().into());

    // Real-to-padding transition: current row must be last.
    let real_to_padding: AB::Expr =
        local.is_real.clone().into() * (AB::Expr::ONE - next.is_real.clone().into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last.clone().into()));
}
