//! AIR constraints for the sealed relation table lane.
use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, num_cols};
use tabula_stark::air::interaction::AirInteraction;
use tabula_stark::chips::{ChipId, ChipSpec};

use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::WIDTH;

use super::rows::{RELATION_TABLE_BUS, RELATION_TABLE_CHIP_ID, RELATION_TABLE_DOMAIN_TAG};

pub(super) struct RelationTableCols<T> {
    pub(super) is_real: T,
    pub(super) is_terminal_block: T,
    pub(super) phase_header: T,
    pub(super) phase_row0: T,
    pub(super) phase_row1: T,
    pub(super) phase_row2: T,
    pub(super) row_count: T,
    pub(super) relation_id: T,
    pub(super) input_digest: [T; 8],
    pub(super) output_digest: [T; 8],
    pub(super) lookup_mult: T,
    pub(super) prev_digest: [T; 8],
    pub(super) block_values: [T; 8],
    pub(super) perm_state_out: [T; WIDTH],
    pub(super) poseidon: PoseidonCols<T>,
}

pub(super) const fn relation_table_width() -> usize {
    num_cols::<RelationTableCols<u8>, u8>()
}

#[derive(Clone, Debug)]
pub(super) struct RelationTableRoundRow {
    pub(super) is_terminal_block: bool,
    pub(super) phase_header: bool,
    pub(super) phase_row0: bool,
    pub(super) phase_row1: bool,
    pub(super) phase_row2: bool,
    pub(super) row_count: u32,
    pub(super) relation_id: u32,
    pub(super) input_digest: [u32; 8],
    pub(super) output_digest: [u32; 8],
    pub(super) lookup_mult: u32,
    pub(super) prev_digest: [u32; 8],
    pub(super) block_values: [KoalaBear; 8],
    pub(super) perm_state_out: [KoalaBear; WIDTH],
    pub(super) round_ctr: u32,
    pub(super) round_data: crate::poseidon::constants::PoseidonRoundData,
    pub(super) perm_input: [KoalaBear; WIDTH],
    pub(super) perm_output: [KoalaBear; 8],
}

/// Relation lookup AIR chip.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelationTableChip;

impl ChipSpec for RelationTableChip {
    fn chip_id(&self) -> ChipId {
        RELATION_TABLE_CHIP_ID
    }

    fn preprocessed_width(&self) -> usize {
        POSEIDON_PREPROCESSED_WIDTH
    }
}

impl<F> BaseAir<F> for RelationTableChip {
    fn width(&self) -> usize {
        relation_table_width()
    }

    fn num_public_values(&self) -> usize {
        8
    }

    fn preprocessed_next_row_columns(&self) -> Vec<usize> {
        vec![]
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for RelationTableChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &RelationTableCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &RelationTableCols<AB::Var> = borrow_cols(main.next_slice());
        let poseidon_local = &local.poseidon;
        let poseidon_next = &next.poseidon;

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();
        let not_last_round: AB::Expr = AB::Expr::ONE - poseidon_local.is_last_round.into();

        builder.assert_zero(local.is_real.into() - poseidon_local.is_real.into());
        builder.assert_zero(next.is_real.into() - poseidon_next.is_real.into());
        poseidon_air::constrain_booleans(builder, poseidon_local);
        poseidon_air::constrain_sbox_element0(builder, poseidon_local, is_real.clone());
        poseidon_air::constrain_sbox_full_round(
            builder,
            poseidon_local,
            is_real.clone(),
            poseidon_local.is_full_round.into(),
        );
        poseidon_air::constrain_linear_layer_full(
            builder,
            poseidon_local,
            poseidon_next,
            is_real.clone()
                * poseidon_local.is_full_round.into()
                * (AB::Expr::ONE - poseidon_local.is_last_round.into()),
        );
        poseidon_air::constrain_linear_layer_partial(
            builder,
            poseidon_local,
            poseidon_next,
            is_real.clone()
                * (AB::Expr::ONE - poseidon_local.is_full_round.into())
                * (AB::Expr::ONE - poseidon_local.is_last_round.into()),
        );
        poseidon_air::constrain_round_control(
            builder,
            poseidon_local,
            poseidon_next,
            both_real.clone(),
        );
        poseidon_air::constrain_perm_output(
            builder,
            poseidon_local,
            poseidon_next,
            is_real.clone(),
            both_real.clone(),
        );
        poseidon_air::constrain_round_constants(builder, poseidon_local, is_real.clone());

        constrain_is_real_prefix(builder, local.is_real, next.is_real);
        builder.assert_bool(local.is_terminal_block);
        builder.assert_bool(local.phase_header);
        builder.assert_bool(local.phase_row0);
        builder.assert_bool(local.phase_row1);
        builder.assert_bool(local.phase_row2);

        let phase_sum: AB::Expr = local.phase_header.into()
            + local.phase_row0.into()
            + local.phase_row1.into()
            + local.phase_row2.into();
        builder.assert_zero(is_real.clone() * (phase_sum.clone() - AB::Expr::ONE));
        builder
            .when_first_row()
            .assert_zero(is_real.clone() * (AB::Expr::ONE - local.phase_header.into()));
        let end_real: AB::Expr = is_real.clone() * (AB::Expr::ONE - next.is_real.into());
        builder
            .when_transition()
            .assert_zero(end_real.clone() * (AB::Expr::ONE - poseidon_local.is_last_round.into()));
        builder
            .when_transition()
            .assert_zero(end_real.clone() * (AB::Expr::ONE - local.is_terminal_block.into()));

        let carry_gate: AB::Expr = both_real.clone() * not_last_round.clone();
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.block_values[idx].into() - local.block_values[idx].into()),
            );
            builder.when_transition().assert_zero(
                carry_gate.clone() * (next.prev_digest[idx].into() - local.prev_digest[idx].into()),
            );
        }
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.input_digest[idx].into() - local.input_digest[idx].into()),
            );
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.output_digest[idx].into() - local.output_digest[idx].into()),
            );
        }
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.row_count.into() - local.row_count.into()));
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.relation_id.into() - local.relation_id.into()));
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.lookup_mult.into() - local.lookup_mult.into()));
        builder.when_transition().assert_zero(
            carry_gate.clone() * (next.is_terminal_block.into() - local.is_terminal_block.into()),
        );
        builder.when_transition().assert_zero(
            carry_gate.clone() * (next.phase_header.into() - local.phase_header.into()),
        );
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.phase_row0.into() - local.phase_row0.into()));
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.phase_row1.into() - local.phase_row1.into()));
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.phase_row2.into() - local.phase_row2.into()));

        let terminal_gate: AB::Expr = both_real.clone()
            * poseidon_local.is_last_round.into()
            * local.is_terminal_block.into();
        builder
            .when_transition()
            .assert_zero(terminal_gate * next.is_real.into());

        let continue_gate: AB::Expr = both_real.clone()
            * poseidon_local.is_last_round.into()
            * (AB::Expr::ONE - local.is_terminal_block.into());
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                continue_gate.clone()
                    * (next.prev_digest[idx].into() - local.perm_state_out[idx].into()),
            );
        }
        builder.when_transition().assert_zero(
            continue_gate.clone()
                * local.phase_header.into()
                * (AB::Expr::ONE - next.phase_row0.into()),
        );
        builder.when_transition().assert_zero(
            continue_gate.clone()
                * local.phase_row0.into()
                * (AB::Expr::ONE - next.phase_row1.into()),
        );
        builder.when_transition().assert_zero(
            continue_gate.clone()
                * local.phase_row1.into()
                * (AB::Expr::ONE - next.phase_row2.into()),
        );
        builder.when_transition().assert_zero(
            continue_gate.clone()
                * local.phase_row2.into()
                * (AB::Expr::ONE - next.phase_row0.into()),
        );
        builder.when_transition().assert_zero(
            continue_gate.clone() * local.phase_header.into() * next.phase_header.into(),
        );
        builder
            .when_transition()
            .assert_zero(continue_gate.clone() * local.phase_row0.into() * next.phase_row0.into());
        builder
            .when_transition()
            .assert_zero(continue_gate.clone() * local.phase_row1.into() * next.phase_row1.into());
        builder
            .when_transition()
            .assert_zero(continue_gate.clone() * local.phase_row2.into() * next.phase_row2.into());

        let keep_same_row: AB::Expr =
            continue_gate.clone() * (local.phase_row0.into() + local.phase_row1.into());
        builder.when_transition().assert_zero(
            keep_same_row.clone() * (next.relation_id.into() - local.relation_id.into()),
        );
        builder.when_transition().assert_zero(
            keep_same_row.clone() * (next.lookup_mult.into() - local.lookup_mult.into()),
        );
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                keep_same_row.clone()
                    * (next.input_digest[idx].into() - local.input_digest[idx].into()),
            );
            builder.when_transition().assert_zero(
                keep_same_row.clone()
                    * (next.output_digest[idx].into() - local.output_digest[idx].into()),
            );
        }

        let first_round_gate: AB::Expr = is_real.clone() * poseidon_local.is_first_round.into();
        for idx in 0..8 {
            builder.assert_zero(
                first_round_gate.clone()
                    * (poseidon_local.perm_input[idx].into() - local.prev_digest[idx].into()),
            );
            builder.assert_zero(
                first_round_gate.clone()
                    * (poseidon_local.perm_input[8 + idx].into() - local.block_values[idx].into()),
            );
        }

        let sbox_out: [AB::Expr; WIDTH] =
            core::array::from_fn(|idx| poseidon_local.sbox_y3[idx].into());
        let expected_state_out = poseidon_air::external_linear_exprs::<AB>(sbox_out);
        let verify_last_round: AB::Expr = is_real.clone() * poseidon_local.is_last_round.into();
        for (idx, expected) in expected_state_out.iter().enumerate() {
            builder.assert_zero(
                verify_last_round.clone() * (local.perm_state_out[idx].into() - expected.clone()),
            );
        }

        for idx in 0..8 {
            let expected = expected_block_value::<AB>(local, idx);
            builder.assert_zero(is_real.clone() * (local.block_values[idx].into() - expected));
        }

        let lookup_mult: AB::Expr =
            first_round_gate.clone() * local.phase_row0.into() * local.lookup_mult.into();
        let mut values = Vec::with_capacity(17);
        values.push(local.relation_id.into());
        for idx in 0..8 {
            values.push(local.input_digest[idx].into());
        }
        for idx in 0..8 {
            values.push(local.output_digest[idx].into());
        }
        builder.receive(AirInteraction {
            values,
            multiplicity: lookup_mult,
            bus: RELATION_TABLE_BUS,
        });

        let terminal_digest_gate: AB::Expr = verify_last_round * local.is_terminal_block.into();
        let pvs = builder.public_values().to_vec();
        for (idx, public_value) in pvs.iter().enumerate().take(8) {
            builder.assert_zero(
                terminal_digest_gate.clone()
                    * (local.perm_state_out[idx].into() - (*public_value).into()),
            );
        }
    }
}

fn expected_block_value<AB: AirBuilder>(
    local: &RelationTableCols<AB::Var>,
    value_index: usize,
) -> AB::Expr {
    local.phase_header.into() * header_block_value::<AB>(local, value_index)
        + local.phase_row0.into() * row0_block_value::<AB>(local, value_index)
        + local.phase_row1.into() * row1_block_value::<AB>(local, value_index)
        + local.phase_row2.into() * row2_block_value::<AB>(local, value_index)
}

fn header_block_value<AB: AirBuilder>(
    local: &RelationTableCols<AB::Var>,
    index: usize,
) -> AB::Expr {
    match index {
        0 => expr_from_u32::<AB>(RELATION_TABLE_DOMAIN_TAG),
        1 => local.row_count.into(),
        _ => AB::Expr::ZERO,
    }
}

fn row0_block_value<AB: AirBuilder>(local: &RelationTableCols<AB::Var>, index: usize) -> AB::Expr {
    match index {
        0 => local.relation_id.into(),
        1..=7 => local.input_digest[index - 1].into(),
        _ => AB::Expr::ZERO,
    }
}

fn row1_block_value<AB: AirBuilder>(local: &RelationTableCols<AB::Var>, index: usize) -> AB::Expr {
    match index {
        0 => local.input_digest[7].into(),
        1..=7 => local.output_digest[index - 1].into(),
        _ => AB::Expr::ZERO,
    }
}

fn row2_block_value<AB: AirBuilder>(local: &RelationTableCols<AB::Var>, index: usize) -> AB::Expr {
    match index {
        0 => local.output_digest[7].into(),
        _ => AB::Expr::ZERO,
    }
}
