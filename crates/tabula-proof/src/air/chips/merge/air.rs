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
//! Hash chain input composition and LogUp declarations are deferred to M9.

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero, constrain_strict_ineq};

use super::columns::{GlobalMergeCols, merge_width};

/// The GlobalMerge AIR chip, generic over value width.
#[derive(Debug)]
pub struct GlobalMergeChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for GlobalMergeChip<W> {
    fn width(&self) -> usize {
        merge_width::<W>()
    }
}

impl<AB: AirBuilder, const W: usize> Air<AB> for GlobalMergeChip<W> {
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
        constrain_hash_acc_carry(builder, local, next, both_real);
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

/// 8. Hash accumulator carry: within same segment, deleted rows carry hash_acc forward.
///
/// When the current row has `in_new=0` (delete), the next row's hash_acc must
/// equal the current row's hash_acc (within the same segment).
///
/// `both_real · (1 − tc_changed) · (1 − in_new) · (next.hash_acc[j] − local.hash_acc[j]) = 0`
fn constrain_hash_acc_carry<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &GlobalMergeCols<AB::Var, W>,
    next: &GlobalMergeCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    let same_segment: AB::Expr = AB::Expr::ONE - local.tc_changed.clone().into();
    let not_in_new: AB::Expr = AB::Expr::ONE - local.in_new.clone().into();
    let gate: AB::Expr = both_real * same_segment * not_in_new;

    for j in 0..8 {
        let diff: AB::Expr = next.hash_acc[j].clone().into() - local.hash_acc[j].clone().into();
        builder.when_transition().assert_zero(gate.clone() * diff);
    }
}
