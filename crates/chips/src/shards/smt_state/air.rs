//! SmtStateShardChip — AIR constraints for per-column row-level SMT proofs.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::primitives::DOMAIN_SMT;
use tabula_gadgets::{
    constrain_constant_identity, constrain_is_real_prefix, constrain_is_zero, constrain_key_halves,
    integer::expr_from_u32, send_key_range_checks,
};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, CommitmentAirBuilder, PoseidonAirBuilder,
};
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{
    DIGEST_WIDTH, HI_REGION_ROOT_POWER, LOW_REGION_SWITCH_POWER, SmtStateShardCols,
    smt_state_shard_width,
};

/// Per-column state shard AIR chip for SMT-backed columns.
#[derive(Debug, Clone)]
pub struct SmtStateShardChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<const W: usize> SmtStateShardChip<W> {
    /// Create a new SMT state shard chip for a specific column.
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

impl<const W: usize> ChipSpec for SmtStateShardChip<W> {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "SmtStateShard"
    }
}

impl<F, const W: usize> BaseAir<F> for SmtStateShardChip<W> {
    fn width(&self) -> usize {
        smt_state_shard_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for SmtStateShardChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &SmtStateShardCols<AB::Var, W> = borrow_cols(local_row);
        let next: &SmtStateShardCols<AB::Var, W> = borrow_cols(next_row);

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();

        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real, next.is_real);
        constrain_column_constants(builder, local, next, both_real.clone());
        constrain_leaf_hash_inputs(builder, local, is_real.clone());
        constrain_leaf_bindings(builder, local, is_real.clone());
        constrain_path_mux(builder, local, &is_real);
        builder.send_poseidon_perm(
            &local.old_leaf_perm_input,
            &local.old_leaf_hash,
            is_real.clone() * local.is_leaf.into(),
        );
        builder.send_poseidon_perm(
            &local.new_leaf_perm_input,
            &local.new_leaf_hash,
            is_real.clone() * local.is_leaf.into(),
        );
        builder.send_poseidon_perm(&local.old_perm_input, &local.old_parent, is_real.clone());
        builder.send_poseidon_perm(&local.new_perm_input, &local.new_parent, is_real.clone());

        constrain_is_zero(builder, local.is_root.into(), &local.next_is_new_path);
        constrain_key_reconstruction(builder, local, next, &is_real, &both_real);
        constrain_path_structure(builder, local, next, &is_real, &both_real);
        constrain_root_consistency(builder, local, &is_real);
        constrain_key_halves(builder, &local.key);

        builder.receive_base_state_entry(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.old_val,
            local.old_is_null.into(),
            is_real.clone() * local.is_leaf.into() * local.read_mult_witness.into(),
        );
        builder.receive_coalesced_write(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.new_val,
            local.new_is_null.into(),
            is_real.clone() * local.is_leaf.into() * local.write_mult_witness.into(),
        );
        builder.send_commitment(
            local.table_id.into(),
            local.col_id.into(),
            AB::Expr::ZERO,
            local.column_is_touched.into(),
            &local.column_old_root,
            is_real.clone()
                * local.is_root.into()
                * local.root_mult_witness.into()
                * (AB::Expr::ONE - local.column_is_empty_old.into()),
        );
        builder.send_commitment(
            local.table_id.into(),
            local.col_id.into(),
            AB::Expr::ONE,
            local.column_is_touched.into(),
            &local.column_new_root,
            is_real.clone()
                * local.is_root.into()
                * local.root_mult_witness.into()
                * local.column_is_touched.into()
                * (AB::Expr::ONE - local.column_is_empty_new.into()),
        );
        send_key_range_checks(builder, &local.key, is_real);
    }
}

fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
) {
    builder.assert_bool(local.old_is_null);
    builder.assert_bool(local.new_is_null);
    builder.assert_bool(local.read_mult_witness);
    builder.assert_bool(local.write_mult_witness);
    builder.assert_bool(local.column_is_empty_old);
    builder.assert_bool(local.column_is_empty_new);
    builder.assert_bool(local.column_is_touched);
    builder.assert_bool(local.path_bit);
    builder.assert_bool(local.is_leaf);
    builder.assert_bool(local.is_root);
    builder.assert_bool(local.is_hi_region);
    builder.assert_bool(local.root_mult_witness);
}

fn constrain_column_constants<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    next: &SmtStateShardCols<AB::Var, W>,
    both_real: AB::Expr,
) {
    constrain_constant_identity(
        builder,
        local.table_id,
        next.table_id,
        local.col_id,
        next.col_id,
        both_real.clone(),
    );

    for i in 0..DIGEST_WIDTH {
        builder.when_transition().assert_zero(
            both_real.clone() * (next.column_old_root[i].into() - local.column_old_root[i].into()),
        );
        builder.when_transition().assert_zero(
            both_real.clone() * (next.column_new_root[i].into() - local.column_new_root[i].into()),
        );
    }
    builder.when_transition().assert_zero(
        both_real.clone() * (next.column_is_empty_old.into() - local.column_is_empty_old.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * (next.column_is_empty_new.into() - local.column_is_empty_new.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * (next.column_is_touched.into() - local.column_is_touched.into()),
    );

    let within_path: AB::Expr = local.next_is_new_path.is_zero.into();
    let same_path: AB::Expr = both_real * within_path;

    builder.when_transition().assert_zero(
        same_path.clone() * (next.key.limbs.limb0.into() - local.key.limbs.limb0.into()),
    );
    builder.when_transition().assert_zero(
        same_path.clone() * (next.key.limbs.limb1.into() - local.key.limbs.limb1.into()),
    );
    builder.when_transition().assert_zero(
        same_path.clone() * (next.key.limbs.limb2.into() - local.key.limbs.limb2.into()),
    );
    for i in 0..W {
        builder
            .when_transition()
            .assert_zero(same_path.clone() * (next.old_val[i].into() - local.old_val[i].into()));
        builder
            .when_transition()
            .assert_zero(same_path.clone() * (next.new_val[i].into() - local.new_val[i].into()));
    }
    builder
        .when_transition()
        .assert_zero(same_path.clone() * (next.old_is_null.into() - local.old_is_null.into()));
    builder
        .when_transition()
        .assert_zero(same_path.clone() * (next.new_is_null.into() - local.new_is_null.into()));
    builder.when_transition().assert_zero(
        same_path.clone() * (next.read_mult_witness.into() - local.read_mult_witness.into()),
    );
    builder.when_transition().assert_zero(
        same_path * (next.write_mult_witness.into() - local.write_mult_witness.into()),
    );
}

fn constrain_leaf_hash_inputs<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let leaf_gate: AB::Expr = is_real * local.is_leaf.into();
    let old_is_present: AB::Expr = AB::Expr::ONE - local.old_is_null.into();
    let new_is_present: AB::Expr = AB::Expr::ONE - local.new_is_null.into();
    let domain_tag: AB::Expr = expr_from_u32::<AB>(DOMAIN_SMT);

    builder.assert_zero(
        leaf_gate.clone()
            * (local.old_leaf_perm_input[0].into()
                - (local.old_is_null.into() * domain_tag.clone()
                    + old_is_present.clone() * local.old_val[0].into())),
    );
    builder.assert_zero(
        leaf_gate.clone()
            * (local.new_leaf_perm_input[0].into()
                - (local.new_is_null.into() * domain_tag
                    + new_is_present.clone() * local.new_val[0].into())),
    );

    for i in 1..W {
        builder.assert_zero(
            leaf_gate.clone()
                * (local.old_leaf_perm_input[i].into()
                    - old_is_present.clone() * local.old_val[i].into()),
        );
        builder.assert_zero(
            leaf_gate.clone()
                * (local.new_leaf_perm_input[i].into()
                    - new_is_present.clone() * local.new_val[i].into()),
        );
    }
    for i in W..16 {
        builder.assert_zero(leaf_gate.clone() * local.old_leaf_perm_input[i].into());
        builder.assert_zero(leaf_gate.clone() * local.new_leaf_perm_input[i].into());
    }
}

fn constrain_leaf_bindings<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let leaf_gate: AB::Expr = is_real * local.is_leaf.into();
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            leaf_gate.clone() * (local.old_node[i].into() - local.old_leaf_hash[i].into()),
        );
        builder.assert_zero(
            leaf_gate.clone() * (local.new_node[i].into() - local.new_leaf_hash[i].into()),
        );
    }
}

fn constrain_path_mux<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    is_real: &AB::Expr,
) {
    let bit: AB::Expr = local.path_bit.into();
    let not_bit: AB::Expr = AB::Expr::ONE - bit.clone();

    for i in 0..DIGEST_WIDTH {
        let expected_old_left: AB::Expr =
            not_bit.clone() * local.old_node[i].into() + bit.clone() * local.old_sibling[i].into();
        let expected_old_right: AB::Expr =
            bit.clone() * local.old_node[i].into() + not_bit.clone() * local.old_sibling[i].into();
        let expected_new_left: AB::Expr =
            not_bit.clone() * local.new_node[i].into() + bit.clone() * local.new_sibling[i].into();
        let expected_new_right: AB::Expr =
            bit.clone() * local.new_node[i].into() + not_bit.clone() * local.new_sibling[i].into();

        builder.assert_zero(is_real.clone() * (local.old_perm_input[i].into() - expected_old_left));
        builder.assert_zero(
            is_real.clone() * (local.old_perm_input[8 + i].into() - expected_old_right),
        );
        builder.assert_zero(is_real.clone() * (local.new_perm_input[i].into() - expected_new_left));
        builder.assert_zero(
            is_real.clone() * (local.new_perm_input[8 + i].into() - expected_new_right),
        );
    }
}

fn constrain_key_reconstruction<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    next: &SmtStateShardCols<AB::Var, W>,
    is_real: &AB::Expr,
    both_real: &AB::Expr,
) {
    let switch_val: AB::Expr =
        local.low_level_power.into() - expr_from_u32::<AB>(LOW_REGION_SWITCH_POWER);
    constrain_is_zero(builder, switch_val, &local.switch_level_iz);
    let root_val: AB::Expr =
        local.hi_level_power.into() - expr_from_u32::<AB>(HI_REGION_ROOT_POWER);
    constrain_is_zero(builder, root_val, &local.root_level_iz);

    builder.assert_zero(is_real.clone() * local.is_leaf.into() * local.is_hi_region.into());
    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - local.is_hi_region.into()) * local.hi_key_acc.into(),
    );
    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - local.is_hi_region.into()) * local.hi_level_power.into(),
    );

    builder.assert_zero(
        is_real.clone() * local.is_leaf.into() * (local.low_key_acc.into() - local.path_bit.into()),
    );
    builder.assert_zero(
        is_real.clone() * local.is_leaf.into() * (local.low_level_power.into() - AB::Expr::ONE),
    );

    builder.assert_zero(
        is_real.clone() * local.is_root.into() * (AB::Expr::ONE - local.is_hi_region.into()),
    );
    builder.assert_zero(
        is_real.clone()
            * local.is_root.into()
            * (AB::Expr::ONE - local.root_level_iz.is_zero.into()),
    );

    let within_path: AB::Expr = local.next_is_new_path.is_zero.into();
    let low_region: AB::Expr = AB::Expr::ONE - local.is_hi_region.into();
    let switch: AB::Expr = low_region.clone() * local.switch_level_iz.is_zero.into();
    let low_cont: AB::Expr =
        low_region.clone() * (AB::Expr::ONE - local.switch_level_iz.is_zero.into());
    let hi_cont: AB::Expr = local.is_hi_region.into();

    builder.when_transition().assert_zero(
        both_real.clone() * within_path.clone() * low_cont.clone() * next.is_hi_region.into(),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * low_cont.clone()
            * (next.low_level_power.into() - local.low_level_power.into() * AB::Expr::TWO),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * low_cont.clone()
            * (next.low_key_acc.into()
                - local.low_key_acc.into()
                - next.path_bit.into() * next.low_level_power.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * within_path.clone() * low_cont.clone() * next.hi_key_acc.into(),
    );
    builder.when_transition().assert_zero(
        both_real.clone() * within_path.clone() * low_cont * next.hi_level_power.into(),
    );

    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * switch.clone()
            * (next.is_hi_region.into() - AB::Expr::ONE),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * switch.clone()
            * (next.low_key_acc.into() - local.low_key_acc.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * switch.clone()
            * (next.low_level_power.into() - local.low_level_power.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * switch.clone()
            * (next.hi_key_acc.into() - next.path_bit.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * switch
            * (next.hi_level_power.into() - AB::Expr::ONE),
    );

    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * hi_cont.clone()
            * (next.is_hi_region.into() - AB::Expr::ONE),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * hi_cont.clone()
            * (next.low_key_acc.into() - local.low_key_acc.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * hi_cont.clone()
            * (next.low_level_power.into() - local.low_level_power.into()),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path.clone()
            * hi_cont.clone()
            * (next.hi_level_power.into() - local.hi_level_power.into() * AB::Expr::TWO),
    );
    builder.when_transition().assert_zero(
        both_real.clone()
            * within_path
            * hi_cont
            * (next.hi_key_acc.into()
                - local.hi_key_acc.into()
                - next.path_bit.into() * next.hi_level_power.into()),
    );

    builder.assert_zero(
        is_real.clone()
            * local.is_root.into()
            * (local.key.limbs.limb0.into() - local.low_key_acc.into()),
    );
    builder.assert_zero(
        is_real.clone()
            * local.is_root.into()
            * (local.key.limbs.limb1.into() - local.hi_key_acc.into()),
    );
    builder.assert_zero(is_real.clone() * local.is_root.into() * local.key.limbs.limb2.into());
}

fn constrain_path_structure<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    next: &SmtStateShardCols<AB::Var, W>,
    is_real: &AB::Expr,
    both_real: &AB::Expr,
) {
    builder
        .when_first_row()
        .assert_zero(is_real.clone() * (AB::Expr::ONE - local.is_leaf.into()));

    builder.assert_zero(
        is_real.clone() * (AB::Expr::ONE - local.is_root.into()) * local.root_mult_witness.into(),
    );

    let within_path: AB::Expr = local.next_is_new_path.is_zero.into();
    let at_boundary: AB::Expr = AB::Expr::ONE - within_path.clone();

    builder
        .when_transition()
        .assert_zero(both_real.clone() * within_path.clone() * next.is_leaf.into());
    builder.when_transition().assert_zero(
        both_real.clone() * at_boundary.clone() * (AB::Expr::ONE - next.is_leaf.into()),
    );
    builder
        .when_transition()
        .assert_zero(both_real.clone() * at_boundary * local.root_mult_witness.into());
    builder.when_transition().assert_zero(
        is_real.clone()
            * (AB::Expr::ONE - next.is_real.into())
            * (AB::Expr::ONE - local.is_root.into()),
    );
    builder.when_transition().assert_zero(
        is_real.clone()
            * (AB::Expr::ONE - next.is_real.into())
            * (AB::Expr::ONE - local.root_mult_witness.into()),
    );

    for i in 0..DIGEST_WIDTH {
        builder.when_transition().assert_zero(
            both_real.clone()
                * within_path.clone()
                * (next.old_node[i].into() - local.old_parent[i].into()),
        );
        builder.when_transition().assert_zero(
            both_real.clone()
                * within_path.clone()
                * (next.new_node[i].into() - local.new_parent[i].into()),
        );
    }
}

fn constrain_root_consistency<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &SmtStateShardCols<AB::Var, W>,
    is_real: &AB::Expr,
) {
    let root_gate: AB::Expr = is_real.clone() * local.is_root.into();
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            root_gate.clone() * (local.old_parent[i].into() - local.column_old_root[i].into()),
        );
        builder.assert_zero(
            root_gate.clone() * (local.new_parent[i].into() - local.column_new_root[i].into()),
        );
    }
}
