//! AIR constraints for the IR-hash transcript family.
#![allow(unused_imports)]

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::{NativeDigest, PoseidonHasher};
use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_core::traits::{DOMAIN_TAG_HASH_IR, Hasher};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::air::interaction::{AirInteraction, BusId};
use tabula_stark::chips::{ChipId, ChipSpec};
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::{TOTAL_ROUNDS, WIDTH, poseidon2_permutation};
use crate::poseidon::generate_poseidon_preprocessed;

use super::call::{IR_HASH_BUS, IR_HASH_CHIP_ID, IR_HASH_RATE};

pub(super) struct IrHashCols<T> {
    pub(super) tx_index: T,
    pub(super) instruction_index: T,
    pub(super) is_first_block: T,
    pub(super) is_last_block: T,
    pub(super) block_byte_real: [T; IR_HASH_RATE],
    pub(super) block_bytes: [T; IR_HASH_RATE],
    pub(super) perm_state_out: [T; WIDTH],
    pub(super) poseidon: PoseidonCols<T>,
}

pub(super) const fn ir_hash_width() -> usize {
    num_cols::<IrHashCols<u8>, u8>()
}

#[derive(Clone, Debug)]
pub(super) struct IrHashRoundRow {
    pub(super) tx_index: u32,
    pub(super) instruction_index: u32,
    pub(super) is_first_block: bool,
    pub(super) is_last_block: bool,
    pub(super) block_byte_real: [bool; IR_HASH_RATE],
    pub(super) block_bytes: [u8; IR_HASH_RATE],
    pub(super) perm_state_out: [KoalaBear; WIDTH],
    pub(super) round_ctr: u32,
    pub(super) round_data: crate::poseidon::constants::PoseidonRoundData,
    pub(super) perm_input: [KoalaBear; WIDTH],
    pub(super) perm_output: [KoalaBear; 8],
}

/// Dedicated chip proving canonical portable IR hash semantics.
#[derive(Clone, Copy, Debug, Default)]
pub struct IrHashChip;

impl ChipSpec for IrHashChip {
    fn chip_id(&self) -> ChipId {
        IR_HASH_CHIP_ID
    }

    fn preprocessed_width(&self) -> usize {
        POSEIDON_PREPROCESSED_WIDTH
    }
}

impl<F> BaseAir<F> for IrHashChip {
    fn width(&self) -> usize {
        ir_hash_width()
    }

    fn preprocessed_next_row_columns(&self) -> Vec<usize> {
        vec![]
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for IrHashChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &IrHashCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &IrHashCols<AB::Var> = borrow_cols(main.next_slice());
        let poseidon_local = &local.poseidon;
        let poseidon_next = &next.poseidon;

        let is_real: AB::Expr = poseidon_local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * poseidon_next.is_real.into();
        let not_last_round: AB::Expr = AB::Expr::ONE - poseidon_local.is_last_round.into();

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

        builder.assert_bool(local.is_first_block);
        builder.assert_bool(local.is_last_block);
        for idx in 0..IR_HASH_RATE {
            builder.assert_bool(local.block_byte_real[idx]);
        }

        builder
            .when_first_row()
            .assert_zero(is_real.clone() * (AB::Expr::ONE - local.is_first_block.into()));

        for idx in 0..IR_HASH_RATE - 1 {
            builder.assert_zero(
                is_real.clone()
                    * local.block_byte_real[idx + 1].into()
                    * (AB::Expr::ONE - local.block_byte_real[idx].into()),
            );
        }
        builder.assert_zero(is_real.clone() * (AB::Expr::ONE - local.block_byte_real[0].into()));

        let carry_gate: AB::Expr = both_real.clone() * not_last_round.clone();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * (next.tx_index.into() - local.tx_index.into()));
        builder.when_transition().assert_zero(
            carry_gate.clone() * (next.instruction_index.into() - local.instruction_index.into()),
        );
        builder.when_transition().assert_zero(
            carry_gate.clone() * (next.is_first_block.into() - local.is_first_block.into()),
        );
        builder.when_transition().assert_zero(
            carry_gate.clone() * (next.is_last_block.into() - local.is_last_block.into()),
        );
        for idx in 0..IR_HASH_RATE {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.block_byte_real[idx].into() - local.block_byte_real[idx].into()),
            );
            builder.when_transition().assert_zero(
                carry_gate.clone() * (next.block_bytes[idx].into() - local.block_bytes[idx].into()),
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
        for idx in 0..WIDTH {
            builder.when_transition().assert_zero(
                carry_gate.clone()
                    * (next.perm_state_out[idx].into() - local.perm_state_out[idx].into()),
            );
        }

        let first_block_gate: AB::Expr =
            is_real.clone() * poseidon_local.is_first_round.into() * local.is_first_block.into();
        for idx in 0..IR_HASH_RATE {
            let is_real_byte: AB::Expr = local.block_byte_real[idx].into();
            builder.assert_zero(
                first_block_gate.clone()
                    * is_real_byte.clone()
                    * (poseidon_local.perm_input[idx].into() - local.block_bytes[idx].into()),
            );
            builder.assert_zero(
                first_block_gate.clone()
                    * (AB::Expr::ONE - is_real_byte)
                    * poseidon_local.perm_input[idx].into(),
            );
        }
        for idx in IR_HASH_RATE..WIDTH {
            builder.assert_zero(first_block_gate.clone() * poseidon_local.perm_input[idx].into());
        }

        let continue_block_gate: AB::Expr = both_real.clone()
            * poseidon_local.is_last_round.into()
            * (AB::Expr::ONE - local.is_last_block.into());
        builder
            .when_transition()
            .assert_zero(continue_block_gate.clone() * next.is_first_block.into());
        builder.when_transition().assert_zero(
            continue_block_gate.clone() * (next.tx_index.into() - local.tx_index.into()),
        );
        builder.when_transition().assert_zero(
            continue_block_gate.clone()
                * (next.instruction_index.into() - local.instruction_index.into()),
        );
        for idx in 0..IR_HASH_RATE {
            let next_real_byte: AB::Expr = next.block_byte_real[idx].into();
            let expected = next_real_byte.clone() * next.block_bytes[idx].into()
                + (AB::Expr::ONE - next_real_byte) * local.perm_state_out[idx].into();
            builder.when_transition().assert_zero(
                continue_block_gate.clone() * (poseidon_next.perm_input[idx].into() - expected),
            );
        }
        for idx in IR_HASH_RATE..WIDTH {
            builder.when_transition().assert_zero(
                continue_block_gate.clone()
                    * (poseidon_next.perm_input[idx].into() - local.perm_state_out[idx].into()),
            );
        }

        let next_call_gate: AB::Expr =
            both_real.clone() * poseidon_local.is_last_round.into() * local.is_last_block.into();
        builder
            .when_transition()
            .assert_zero(next_call_gate * (next.is_first_block.into() - AB::Expr::ONE));

        let relay_mult: AB::Expr =
            is_real * poseidon_local.is_last_round.into() * local.is_last_block.into();
        let mut relay_values: Vec<AB::Expr> = Vec::with_capacity(10);
        relay_values.push(local.tx_index.into());
        relay_values.push(local.instruction_index.into());
        for idx in 0..8 {
            relay_values.push(poseidon_local.perm_output[idx].into());
        }
        builder.receive(AirInteraction {
            values: relay_values,
            multiplicity: relay_mult,
            bus: IR_HASH_BUS,
        });
    }
}
