//! SSMC property AIR.
//!
//! This chip verifies `PropertyRead` claims against SSMC old-state anchors.
//! Execution sends the full query claim on the `PROPERTY_READ` bus. The SSMC
//! state/meta tiers provide either:
//! - one old-entry anchor with local adjacency metadata, or
//! - an empty-old-column witness.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::{
    U64Limbs, constrain_constant_identity, constrain_is_real_prefix, constrain_key_halves,
    constrain_ordering_halves, constrain_strict_ineq, integer::expr_from_u32,
    send_ordering_range_checks,
};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    EmptyOldColumnAirBuilder, PropertyReadAirBuilder, SsmcOldEntryAirBuilder,
};
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{LessOrEqChecked, SsmcPropertyCols, ssmc_property_width};

/// Per-column SSMC property verifier.
#[derive(Debug, Clone)]
pub struct SsmcPropertyChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<const W: usize> SsmcPropertyChip<W> {
    /// Create a new chip for one `(table_id, col_id)` pair.
    pub fn new(chip_id: ChipId, table_id: u32, col_id: u16) -> Self {
        Self {
            chip_id,
            table_id,
            col_id,
        }
    }

    /// Table identifier this property shard verifies.
    pub fn table_id(&self) -> u32 {
        self.table_id
    }

    /// Column identifier this property shard verifies.
    pub fn col_id(&self) -> u16 {
        self.col_id
    }
}

impl<const W: usize> ChipSpec for SsmcPropertyChip<W> {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "SsmcProperty"
    }
}

impl<F, const W: usize> BaseAir<F> for SsmcPropertyChip<W> {
    fn width(&self) -> usize {
        ssmc_property_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for SsmcPropertyChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &SsmcPropertyCols<AB::Var, W> = borrow_cols(local_row);
        let next: &SsmcPropertyCols<AB::Var, W> = borrow_cols(next_row);

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();

        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real, next.is_real);
        constrain_constant_identity(
            builder,
            local.table_id,
            next.table_id,
            local.col_id,
            next.col_id,
            both_real,
        );

        constrain_key_halves(builder, &local.query_arg0);
        constrain_key_halves(builder, &local.query_arg1);
        constrain_key_halves(builder, &local.result_key);
        constrain_key_halves(builder, &local.anchor_key);
        constrain_key_halves(builder, &local.prev_old_key);
        constrain_key_halves(builder, &local.next_old_key);

        constrain_query_selectors(builder, local, is_real.clone());
        constrain_witness_routing(builder, local, is_real.clone());
        constrain_empty_witness_canonicality(builder, local, is_real.clone());

        let result_non_null: AB::Expr = AB::Expr::ONE - local.result_is_null.into();
        let uses_anchor: AB::Expr = local.uses_anchor.into();
        let uses_empty_old: AB::Expr = local.uses_empty_old.into();
        let has_prev: AB::Expr = local.has_prev_old.into();
        let has_next: AB::Expr = AB::Expr::ONE - local.is_last_old.into();

        let is_successor: AB::Expr = local.query_is_successor.into();
        let is_predecessor: AB::Expr = local.query_is_predecessor.into();

        let succ_non_null = is_real.clone() * is_successor.clone() * result_non_null.clone();
        let pred_non_null = is_real.clone() * is_predecessor.clone() * result_non_null.clone();

        let succ_null = is_real.clone() * is_successor * local.result_is_null.into();
        let pred_null = is_real.clone() * is_predecessor * local.result_is_null.into();

        // Successor proofs.
        require_flag(builder, &local.uses_anchor, succ_non_null.clone(), true);
        require_result_equals_anchor(builder, local, &succ_non_null);
        constrain_strict_compare(
            builder,
            &local.query_arg0.limbs,
            &local.anchor_key.limbs,
            &local.query_lt_anchor,
            &succ_non_null,
        );
        constrain_leq_compare(
            builder,
            &local.prev_old_key.limbs,
            &local.query_arg0.limbs,
            &local.prev_le_query,
            &(succ_non_null.clone() * has_prev.clone()),
        );
        let succ_null_anchor = succ_null.clone() * uses_anchor.clone();
        let succ_null_empty = succ_null * uses_empty_old.clone();
        require_flag(builder, &local.is_last_old, succ_null_anchor.clone(), true);
        constrain_leq_compare(
            builder,
            &local.anchor_key.limbs,
            &local.query_arg0.limbs,
            &local.anchor_le_query,
            &succ_null_anchor,
        );
        require_flag(builder, &local.uses_empty_old, succ_null_empty, true);

        // Predecessor proofs.
        require_flag(builder, &local.uses_anchor, pred_non_null.clone(), true);
        require_result_equals_anchor(builder, local, &pred_non_null);
        constrain_strict_compare(
            builder,
            &local.anchor_key.limbs,
            &local.query_arg0.limbs,
            &local.anchor_lt_query,
            &pred_non_null,
        );
        constrain_leq_compare(
            builder,
            &local.query_arg0.limbs,
            &local.next_old_key.limbs,
            &local.query_le_next,
            &(pred_non_null.clone() * has_next),
        );
        let pred_null_anchor = pred_null.clone() * uses_anchor;
        let pred_null_empty = pred_null * uses_empty_old;
        require_flag(
            builder,
            &local.has_prev_old,
            pred_null_anchor.clone(),
            false,
        );
        constrain_leq_compare(
            builder,
            &local.query_arg0.limbs,
            &local.anchor_key.limbs,
            &local.query_le_anchor,
            &pred_null_anchor,
        );
        require_flag(builder, &local.uses_empty_old, pred_null_empty, true);

        let query_arg0 = [
            local.query_arg0.limbs.limb0,
            local.query_arg0.limbs.limb1,
            local.query_arg0.limbs.limb2,
        ];
        let query_arg1 = [
            local.query_arg1.limbs.limb0,
            local.query_arg1.limbs.limb1,
            local.query_arg1.limbs.limb2,
        ];
        let result_key = [
            local.result_key.limbs.limb0,
            local.result_key.limbs.limb1,
            local.result_key.limbs.limb2,
        ];

        builder.receive_property_read(
            local.table_id.into(),
            local.col_id.into(),
            local.query_type.into(),
            &query_arg0,
            &query_arg1,
            &local.result_val,
            &result_key,
            local.result_is_null.into(),
            is_real.clone(),
        );

        builder.receive_empty_old_column(
            local.table_id.into(),
            local.col_id.into(),
            is_real.clone() * local.uses_empty_old.into(),
        );

        builder.receive_ssmc_old_entry(
            local.table_id.into(),
            local.col_id.into(),
            &local.anchor_key.limbs,
            &local.anchor_val,
            local.has_prev_old.into(),
            &local.prev_old_key.limbs,
            local.is_last_old.into(),
            &local.next_old_key.limbs,
            is_real * local.uses_anchor.into(),
        );
    }
}

fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SsmcPropertyCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_real);
    builder.assert_bool(local.query_is_successor);
    builder.assert_bool(local.query_is_predecessor);
    builder.assert_bool(local.result_is_null);
    builder.assert_bool(local.uses_empty_old);
    builder.assert_bool(local.uses_anchor);
    builder.assert_bool(local.has_prev_old);
    builder.assert_bool(local.is_last_old);
}

fn constrain_query_selectors<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SsmcPropertyCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let selector_sum: AB::Expr =
        local.query_is_successor.into() + local.query_is_predecessor.into();
    builder.assert_zero(is_real.clone() * (selector_sum - AB::Expr::ONE));

    let encoded: AB::Expr = local.query_is_successor.into() * expr_from_u32::<AB>(2)
        + local.query_is_predecessor.into() * expr_from_u32::<AB>(3);
    builder.assert_zero(is_real * (local.query_type.into() - encoded));
}

fn constrain_witness_routing<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SsmcPropertyCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let route_sum: AB::Expr = local.uses_empty_old.into() + local.uses_anchor.into();
    builder.assert_zero(is_real.clone() * (route_sum - AB::Expr::ONE));
    builder.assert_zero(is_real * local.uses_empty_old.into() * local.uses_anchor.into());
}

fn constrain_empty_witness_canonicality<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SsmcPropertyCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate = is_real * local.uses_empty_old.into();
    for limb in [
        local.anchor_key.limbs.limb0,
        local.anchor_key.limbs.limb1,
        local.anchor_key.limbs.limb2,
        local.prev_old_key.limbs.limb0,
        local.prev_old_key.limbs.limb1,
        local.prev_old_key.limbs.limb2,
        local.next_old_key.limbs.limb0,
        local.next_old_key.limbs.limb1,
        local.next_old_key.limbs.limb2,
    ] {
        builder.assert_zero(gate.clone() * limb.into());
    }
    for i in 0..W {
        builder.assert_zero(gate.clone() * local.anchor_val[i].into());
    }
    builder.assert_zero(gate.clone() * local.has_prev_old.into());
    builder.assert_zero(gate.clone() * local.is_last_old.into());
}

fn require_flag<AB: AirBuilder>(builder: &mut AB, flag: &AB::Var, gate: AB::Expr, expected: bool) {
    let expected_expr = if expected {
        AB::Expr::ONE
    } else {
        AB::Expr::ZERO
    };
    builder.assert_zero(gate * ((*flag).into() - expected_expr));
}

fn require_result_equals_anchor<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SsmcPropertyCols<AB::Var, W>,
    gate: &AB::Expr,
) {
    for i in 0..W {
        builder.assert_zero(
            (*gate).clone() * (local.result_val[i].into() - local.anchor_val[i].into()),
        );
    }
    assert_key_equal(
        builder,
        &local.result_key.limbs,
        &local.anchor_key.limbs,
        gate,
    );
}

fn assert_key_equal<AB: AirBuilder>(
    builder: &mut AB,
    lhs: &U64Limbs<AB::Var>,
    rhs: &U64Limbs<AB::Var>,
    gate: &AB::Expr,
) {
    builder.assert_zero((*gate).clone() * (lhs.limb0.into() - rhs.limb0.into()));
    builder.assert_zero((*gate).clone() * (lhs.limb1.into() - rhs.limb1.into()));
    builder.assert_zero((*gate).clone() * (lhs.limb2.into() - rhs.limb2.into()));
}

fn constrain_strict_compare<AB: InteractionAirBuilder>(
    builder: &mut AB,
    lhs: &U64Limbs<AB::Var>,
    rhs: &U64Limbs<AB::Var>,
    ordering: &tabula_gadgets::OrderingRangeChecked<AB::Var>,
    gate: &AB::Expr,
) {
    {
        let mut when_lt = builder.when((*gate).clone());
        constrain_strict_ineq(&mut when_lt, lhs, rhs, &ordering.ineq);
        constrain_ordering_halves(&mut when_lt, ordering);
    }
    send_ordering_range_checks(builder, ordering, (*gate).clone());
}

fn constrain_leq_compare<AB: InteractionAirBuilder>(
    builder: &mut AB,
    lhs: &U64Limbs<AB::Var>,
    rhs: &U64Limbs<AB::Var>,
    leq: &LessOrEqChecked<AB::Var>,
    gate: &AB::Expr,
) {
    builder.assert_bool(leq.is_eq);
    let eq_gate: AB::Expr = (*gate).clone() * leq.is_eq.into();
    assert_key_equal(builder, lhs, rhs, &eq_gate);

    let lt_gate: AB::Expr = (*gate).clone() * (AB::Expr::ONE - leq.is_eq.into());
    {
        let mut when_lt = builder.when(lt_gate.clone());
        constrain_strict_ineq(&mut when_lt, lhs, rhs, &leq.lt.ineq);
        constrain_ordering_halves(&mut when_lt, &leq.lt);
    }
    send_ordering_range_checks(builder, &leq.lt, lt_gate);
}
