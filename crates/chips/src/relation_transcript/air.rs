//! AIR constraints for the relation transcript family.
use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_contract::format::typed_tuple::{
    TYPED_TUPLE_BLOCKS, TYPED_TUPLE_MAX_SLOTS, TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG,
    TYPED_TUPLE_TRANSCRIPT_RATE, TYPED_TUPLE_VALUE_WIDTH, TypedTupleRole,
};
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, num_cols};
use tabula_stark::air::interaction::AirInteraction;
use tabula_stark::chips::ChipId;
use tabula_stark::chips::ChipSpec;

use crate::execution::{EXECUTION_STANDARD_VALUE_WIDTH, MAX_SLOTS};
use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::WIDTH;

use super::call::{RELATION_DIGEST_BUS, RELATION_TRANSCRIPT_CHIP_ID, RELATION_TUPLE_BUS};

pub(super) struct RelationTranscriptCols<T> {
    pub(super) tx_index: T,
    pub(super) effect_ordinal_in_tx: T,
    pub(super) tuple_role: T,
    pub(super) tuple_used: [T; TYPED_TUPLE_MAX_SLOTS],
    pub(super) tuple_type_ids: [T; TYPED_TUPLE_MAX_SLOTS],
    pub(super) tuple_values: [[T; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    pub(super) block_sel: [T; TYPED_TUPLE_BLOCKS],
    pub(super) block_values: [T; TYPED_TUPLE_TRANSCRIPT_RATE],
    pub(super) prev_digest: [T; 8],
    pub(super) perm_state_out: [T; WIDTH],
    pub(super) poseidon: PoseidonCols<T>,
}

pub(super) const fn relation_transcript_width() -> usize {
    num_cols::<RelationTranscriptCols<u8>, u8>()
}

#[derive(Clone, Debug)]
pub(super) struct RelationTranscriptRoundRow {
    pub(super) tx_index: u32,
    pub(super) effect_ordinal_in_tx: u32,
    pub(super) tuple_role: TypedTupleRole,
    pub(super) tuple_used: [bool; TYPED_TUPLE_MAX_SLOTS],
    pub(super) tuple_type_ids: [u32; TYPED_TUPLE_MAX_SLOTS],
    pub(super) tuple_values: [[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    pub(super) block_index: usize,
    pub(super) block_values: [KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE],
    pub(super) prev_digest: [u32; 8],
    pub(super) perm_state_out: [KoalaBear; WIDTH],
    pub(super) round_ctr: u32,
    pub(super) round_data: crate::poseidon::constants::PoseidonRoundData,
    pub(super) perm_input: [KoalaBear; WIDTH],
    pub(super) perm_output: [KoalaBear; 8],
}

/// Dedicated chip proving typed tuple transcript semantics.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelationTranscriptChip;

impl ChipSpec for RelationTranscriptChip {
    fn chip_id(&self) -> ChipId {
        RELATION_TRANSCRIPT_CHIP_ID
    }

    fn preprocessed_width(&self) -> usize {
        POSEIDON_PREPROCESSED_WIDTH
    }
}

impl<F> BaseAir<F> for RelationTranscriptChip {
    fn width(&self) -> usize {
        relation_transcript_width()
    }

    fn preprocessed_next_row_columns(&self) -> Vec<usize> {
        vec![]
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for RelationTranscriptChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &RelationTranscriptCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &RelationTranscriptCols<AB::Var> = borrow_cols(main.next_slice());
        let poseidon_local = &local.poseidon;
        let poseidon_next = &next.poseidon;

        let is_real: AB::Expr = poseidon_local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * poseidon_next.is_real.into();
        let not_last_round: AB::Expr = AB::Expr::ONE - poseidon_local.is_last_round.into();

        constrain_is_real_prefix(builder, poseidon_local.is_real, poseidon_next.is_real);

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

        for idx in 0..MAX_SLOTS {
            builder.assert_bool(local.tuple_used[idx]);
        }
        for idx in 0..MAX_SLOTS - 1 {
            builder.assert_zero(
                is_real.clone()
                    * local.tuple_used[idx + 1].into()
                    * (AB::Expr::ONE - local.tuple_used[idx].into()),
            );
        }

        let block_sel_sum: AB::Expr = (0..TYPED_TUPLE_BLOCKS)
            .map(|idx| {
                builder.assert_bool(local.block_sel[idx]);
                local.block_sel[idx].into()
            })
            .sum();
        builder.assert_zero(is_real.clone() * (block_sel_sum.clone() - AB::Expr::ONE));

        builder
            .when_first_row()
            .assert_zero(is_real.clone() * (AB::Expr::ONE - local.block_sel[0].into()));
        let end_real: AB::Expr = is_real.clone() * (AB::Expr::ONE - poseidon_next.is_real.into());
        builder
            .when_transition()
            .assert_zero(end_real.clone() * (AB::Expr::ONE - poseidon_local.is_last_round.into()));
        builder.when_transition().assert_zero(
            end_real.clone() * (AB::Expr::ONE - local.block_sel[TYPED_TUPLE_BLOCKS - 1].into()),
        );

        let carry_gate: AB::Expr = both_real.clone() * not_last_round.clone();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.tx_index.into() - local.tx_index.into()));
        builder.when_transition().assert_zero(
            carry_gate.clone()
                * (next.effect_ordinal_in_tx.into() - local.effect_ordinal_in_tx.into()),
        );
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.tuple_role.into() - local.tuple_role.into()));
        for idx in 0..MAX_SLOTS {
            builder.when_transition().assert_zero(
                carry_gate.clone() * (next.tuple_used[idx].into() - local.tuple_used[idx].into()),
            );
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.tuple_type_ids[idx].into() - local.tuple_type_ids[idx].into()),
            );
            for limb in 0..EXECUTION_STANDARD_VALUE_WIDTH {
                builder.when_transition().assert_zero(
                    carry_gate.clone()
                        * (next.tuple_values[idx][limb].into()
                            - local.tuple_values[idx][limb].into()),
                );
            }
        }
        for idx in 0..TYPED_TUPLE_BLOCKS {
            builder.when_transition().assert_zero(
                carry_gate.clone() * (next.block_sel[idx].into() - local.block_sel[idx].into()),
            );
        }
        for idx in 0..TYPED_TUPLE_TRANSCRIPT_RATE {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.block_values[idx].into() - local.block_values[idx].into()),
            );
        }
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                carry_gate.clone() * (next.prev_digest[idx].into() - local.prev_digest[idx].into()),
            );
        }

        let last_block_sel: AB::Expr = local.block_sel[TYPED_TUPLE_BLOCKS - 1].into();
        let block_continue: AB::Expr = both_real.clone()
            * poseidon_local.is_last_round.into()
            * (AB::Expr::ONE - last_block_sel.clone());
        let call_boundary: AB::Expr =
            both_real.clone() * poseidon_local.is_last_round.into() * last_block_sel.clone();

        builder
            .when_transition()
            .assert_zero(block_continue.clone() * next.block_sel[0].into());
        for idx in 1..TYPED_TUPLE_BLOCKS {
            builder.when_transition().assert_zero(
                block_continue.clone()
                    * (next.block_sel[idx].into() - local.block_sel[idx - 1].into()),
            );
        }
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                block_continue.clone()
                    * (next.prev_digest[idx].into() - local.perm_state_out[idx].into()),
            );
        }

        builder
            .when_transition()
            .assert_zero(call_boundary.clone() * (AB::Expr::ONE - next.block_sel[0].into()));
        for idx in 0..8 {
            builder
                .when_transition()
                .assert_zero(call_boundary.clone() * next.prev_digest[idx].into());
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
        for idx in 0..WIDTH {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.perm_state_out[idx].into() - local.perm_state_out[idx].into()),
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

        for idx in 0..TYPED_TUPLE_TRANSCRIPT_RATE {
            let expected = expected_tuple_block_value::<AB>(local, idx);
            builder.assert_zero(is_real.clone() * (local.block_values[idx].into() - expected));
        }

        let arity_expr: AB::Expr = (0..MAX_SLOTS).map(|idx| local.tuple_used[idx].into()).sum();
        builder.assert_zero(
            is_real.clone()
                * local.block_sel[0].into()
                * (local.block_values[0].into()
                    - expr_from_u32::<AB>(TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG)),
        );
        builder.assert_zero(
            is_real.clone()
                * local.block_sel[0].into()
                * (local.block_values[1].into() - local.tuple_role.into()),
        );
        builder.assert_zero(
            is_real.clone()
                * local.block_sel[0].into()
                * (local.block_values[2].into() - arity_expr),
        );

        let tuple_mult: AB::Expr =
            is_real.clone() * poseidon_local.is_first_round.into() * local.block_sel[0].into();
        let mut tuple_values = Vec::with_capacity(
            3 + MAX_SLOTS + MAX_SLOTS + MAX_SLOTS * EXECUTION_STANDARD_VALUE_WIDTH,
        );
        tuple_values.push(local.tx_index.into());
        tuple_values.push(local.effect_ordinal_in_tx.into());
        tuple_values.push(local.tuple_role.into());
        for idx in 0..MAX_SLOTS {
            tuple_values.push(local.tuple_used[idx].into());
        }
        for idx in 0..MAX_SLOTS {
            tuple_values.push(local.tuple_type_ids[idx].into());
        }
        for idx in 0..MAX_SLOTS {
            for limb in 0..EXECUTION_STANDARD_VALUE_WIDTH {
                tuple_values.push(local.tuple_values[idx][limb].into());
            }
        }
        builder.receive(AirInteraction {
            values: tuple_values,
            multiplicity: tuple_mult,
            bus: RELATION_TUPLE_BUS,
        });

        let digest_mult: AB::Expr = verify_last_round * last_block_sel;
        let mut digest_values = Vec::with_capacity(11);
        digest_values.push(local.tx_index.into());
        digest_values.push(local.effect_ordinal_in_tx.into());
        digest_values.push(local.tuple_role.into());
        for idx in 0..8 {
            digest_values.push(local.perm_state_out[idx].into());
        }
        builder.send(AirInteraction {
            values: digest_values,
            multiplicity: digest_mult,
            bus: RELATION_DIGEST_BUS,
        });
    }
}

fn expected_tuple_block_value<AB: AirBuilder>(
    local: &RelationTranscriptCols<AB::Var>,
    value_index: usize,
) -> AB::Expr {
    (0..TYPED_TUPLE_BLOCKS)
        .map(|block_idx| {
            local.block_sel[block_idx].into()
                * schedule_source_expr::<AB>(
                    local,
                    block_idx * TYPED_TUPLE_TRANSCRIPT_RATE + value_index,
                )
        })
        .sum()
}

fn schedule_source_expr<AB: AirBuilder>(
    local: &RelationTranscriptCols<AB::Var>,
    flat_index: usize,
) -> AB::Expr {
    if flat_index == 0 {
        return expr_from_u32::<AB>(TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG);
    }
    if flat_index == 1 {
        return local.tuple_role.into();
    }
    if flat_index == 2 {
        return (0..MAX_SLOTS).map(|idx| local.tuple_used[idx].into()).sum();
    }
    let slot_flat = flat_index - 3;
    let slot = slot_flat / (2 + EXECUTION_STANDARD_VALUE_WIDTH);
    let offset = slot_flat % (2 + EXECUTION_STANDARD_VALUE_WIDTH);
    if slot >= MAX_SLOTS {
        return AB::Expr::ZERO;
    }
    match offset {
        0 => local.tuple_used[slot].into(),
        1 => local.tuple_type_ids[slot].into(),
        limb => local.tuple_values[slot][limb - 2].into(),
    }
}
