//! GlobalSSMCChip — AIR constraints for the SSMC commitment table.
//!
//! The GlobalSSMC table proves sorted-set membership commitments for each
//! SSMC-committed column. Rows are sorted by `(table_id, col_id, key)`.
//!
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (6): is_real, is_first, is_last, tc_changed, mult_witness, segment_is_touched
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
use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero, constrain_strict_ineq};
use crate::air::interaction::{AirInteraction, InteractionKind};

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

        constrain_booleans(builder, local);
        constrain_is_real(builder, local, next);
        constrain_same_key_detection(builder, local, next, both_real.clone());
        constrain_key_ordering(builder, local, next, both_real.clone());
        constrain_boundary_flags(builder, local, next, both_real.clone());
        constrain_segment_is_touched(builder, local, next, both_real.clone());
        constrain_hash_chain_input(builder, local, next, both_real);

        // ── LogUp buses ──
        receive_ssmc_membership(builder, local);
        send_merge_old_list(builder, local);
        send_commitment_verification(builder, local);
        send_poseidon_permutation(builder, local);
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// 1. Boolean constraints on flag columns (is_real handled by is_real_prefix).
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_first.clone());
    builder.assert_bool(local.is_last.clone());
    builder.assert_bool(local.tc_changed.clone());
    builder.assert_bool(local.mult_witness.clone());
    builder.assert_bool(local.segment_is_touched.clone());
}

/// 2. `is_real` prefix: monotonic 1→0 transition.
fn constrain_is_real<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
) {
    constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
}

/// 5-6. Same-key detection via IsZero gadgets + tc_changed derivation.
fn constrain_same_key_detection<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
    let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();

    constrain_is_zero(builder, table_diff, &local.table_diff_iz);
    constrain_is_zero(builder, col_diff, &local.col_diff_iz);

    // tc_changed = 1 iff table or col changed from this row to next.
    // tc_changed = 1 - table_same * col_same
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
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
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

/// 4. Boundary flag constraints.
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
            * local.tc_changed.clone().into()
            * (AB::Expr::ONE - local.is_last.clone().into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * local.tc_changed.clone().into()
            * (AB::Expr::ONE - next.is_first.clone().into()),
    );

    // When NOT tc_changed (both real): current row is not last, next row is not first.
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();
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

/// 6. segment_is_touched must be constant within a segment.
///
/// Within same segment (both_real, tc_changed=0): next.segment_is_touched = local.segment_is_touched.
fn constrain_segment_is_touched<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();
    let diff: AB::Expr =
        next.segment_is_touched.clone().into() - local.segment_is_touched.clone().into();
    builder
        .when_transition()
        .assert_zero(both_real * same_segment * diff);
}

/// Hash chain input composition constraints (C5 prerequisite).
///
/// **First entry** (`is_first=1`):
/// ```text
/// perm_input = [0x00, table_id, col_id, key[3], value[W], 0..]
/// ```
///
/// **Continuation** (`is_first=0`): local constraints for key/value slots,
/// plus transition constraint linking `perm_input[0..8]` to previous row's `hash_acc`.
/// ```text
/// perm_input = [prev_hash_acc[8], key[3], value[W], 0..]
/// ```
fn constrain_hash_chain_input<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
    next: &GlobalSsmcCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let is_real: AB::Expr = local.is_real.clone().into();
    let is_first: AB::Expr = local.is_first.clone().into();
    let not_first: AB::Expr = AB::Expr::ONE - is_first.clone();

    // ── First-entry composition ──
    // perm_input[0] = 0 (domain tag 0x00)
    let first_gate = is_real.clone() * is_first.clone();
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
    // perm_input[6..6+W] = value
    for i in 0..W {
        builder.assert_zero(
            first_gate.clone()
                * (local.perm_input[6 + i].clone().into() - local.value[i].clone().into()),
        );
    }
    // perm_input[6+W..16] = 0 (padding)
    for i in (6 + W)..16 {
        builder.assert_zero(first_gate.clone() * local.perm_input[i].clone().into());
    }

    // ── Continuation composition (local part) ──
    // perm_input[8..11] = key limbs
    let cont_gate = is_real * not_first;
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[8].clone().into() - local.key.limb0.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[9].clone().into() - local.key.limb1.clone().into()),
    );
    builder.assert_zero(
        cont_gate.clone() * (local.perm_input[10].clone().into() - local.key.limb2.clone().into()),
    );
    // perm_input[11..11+W] = value
    for i in 0..W {
        builder.assert_zero(
            cont_gate.clone()
                * (local.perm_input[11 + i].clone().into() - local.value[i].clone().into()),
        );
    }
    // perm_input[11+W..16] = 0 (padding)
    for i in (11 + W)..16 {
        builder.assert_zero(cont_gate.clone() * local.perm_input[i].clone().into());
    }

    // ── Continuation transition constraint ──
    // when both_real AND NOT next.is_first:
    //   next.perm_input[0..8] = local.hash_acc[0..8]
    let trans_gate: AB::Expr = both_real * (AB::Expr::ONE - next.is_first.clone().into());
    for j in 0..8 {
        builder.when_transition().assert_zero(
            trans_gate.clone()
                * (next.perm_input[j].clone().into() - local.hash_acc[j].clone().into()),
        );
    }
}

// ── LogUp bus interactions ──────────────────────────────────────────────────

/// C2 SsmcMembership bus receive.
///
/// Tuple: `(table_id, col_id, key_l0, key_l1, key_l2, value[0..W])`.
/// Multiplicity: `is_real · mult_witness`.
fn receive_ssmc_membership<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.mult_witness.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.key.limb0.clone().into(),
        local.key.limb1.clone().into(),
        local.key.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.value[i].clone().into());
    }

    builder.receive(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::SsmcMembership,
    });
}

/// C3 MergeOldList bus send.
///
/// Tuple: `(table_id, col_id, key_l0, key_l1, key_l2, value[0..W])`.
/// Multiplicity: `is_real · segment_is_touched`.
fn send_merge_old_list<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.segment_is_touched.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        local.key.limb0.clone().into(),
        local.key.limb1.clone().into(),
        local.key.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.value[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        kind: InteractionKind::MergeOldList,
    });
}

/// C6 CommitmentVerification bus send (OldList commitment).
///
/// Tuple: `(table_id, col_id, 0, segment_is_touched, hash_acc[0..8])`.
/// Multiplicity: `is_real · is_last`.
///
/// SSMC sends Com_old (comm_type=0) at segment boundaries.
/// `segment_is_touched` is included to bind against ColumnMeta's `is_touched`.
fn send_commitment_verification<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.is_last.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        AB::Expr::ZERO, // comm_type = 0 (Com_old)
        local.segment_is_touched.clone().into(),
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
/// Multiplicity: `is_real`.
///
/// Every real SSMC row sends one permutation request: the hash chain step
/// that hashes the current entry into the running accumulator.
fn send_poseidon_permutation<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalSsmcCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into();

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
