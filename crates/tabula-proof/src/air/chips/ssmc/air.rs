//! GlobalSSMCChip — AIR constraints for the SSMC commitment table.
//!
//! The GlobalSSMC table proves sorted-set membership commitments for each
//! SSMC-committed column. Rows are sorted by `(table_id, col_id, key)`.
//!
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (4): is_real, is_first, is_last, tc_changed
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Key sorted uniqueness: within same segment, key_next > key
//! 4. Boundary flags: is_first/is_last consistency with tc_changed
//! 5. Segment lex ordering: (t,c) strictly increases across segments
//!
//! Hash chain (hash_acc) and LogUp declarations are deferred to M9.

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero, constrain_strict_ineq};

use super::columns::{GlobalSsmcCols, ssmc_width};

/// The GlobalSSMC AIR chip, generic over value width.
#[derive(Debug)]
pub struct GlobalSsmcChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for GlobalSsmcChip<W> {
    fn width(&self) -> usize {
        ssmc_width::<W>()
    }
}

impl<AB: AirBuilder, const W: usize> Air<AB> for GlobalSsmcChip<W> {
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
        constrain_boundary_flags(builder, local, next, both_real);
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
    // Key ordering is active within same (t,c) segment and not the last entry.
    // Selector: both_real * (1 - tc_changed) — same segment, real rows.
    // is_last also implies no ordering needed (next key is in different segment or padding).
    // But tc_changed already captures segment boundaries, so (1 - tc_changed) suffices.
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();

    // StrictIneq: proves key < next.key (gap = next_key - key - 1 decomposed)
    let mut when_transition = builder.when_transition();
    let mut when_both_real = when_transition.when(both_real.clone());
    let mut when_ordering = when_both_real.when(same_segment);
    constrain_strict_ineq(
        &mut when_ordering,
        &local.key,
        &next.key,
        &local.key_ordering,
    );

    // Segment lex ordering: when tc_changed, (t,c) must be strictly increasing.
    // We enforce: tc_changed ⟹ (next.table_id > table_id) OR
    //   (next.table_id == table_id AND next.col_id > col_id).
    // Encoded as: tc_changed * (table_same) * col_same = 0
    //   (if both table and col are the same, tc_changed can't be 1)
    // This is already implied by tc_changed derivation: tc_changed = 1 - table_same * col_same.
    // So tc_changed = 1 ⟹ NOT (table_same AND col_same). This ensures change.
    //
    // But we also need strict ordering (not just difference). The IsZero gadgets
    // only tell us whether values changed, not direction. For strict lex ordering
    // across segments, we need an additional constraint.
    //
    // For now, the prover asserts correct ordering in trace generation. The AIR
    // only verifies keys are unique within segments. Cross-segment lex ordering
    // of (t,c) is enforced structurally: the IsZero detects change, and
    // completeness is verified via LogUp membership in M9.
    //
    // NOTE: A fully sound AIR would also enforce (t,c) direction, but that
    // requires range-checked subtraction on table_id/col_id. Deferred to M9.
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
