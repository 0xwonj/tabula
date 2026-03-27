//! AIR constraints for the capability transcript family.
use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::air::interaction::{AirInteraction, core_buses};
use tabula_stark::chips::ChipId;

use super::call::{
    CAPABILITY_TRANSCRIPT_CHIP_ID, CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG,
    CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG, CAPABILITY_TRANSCRIPT_WIDTH,
};

pub(super) struct CapabilityTranscriptCols<T> {
    pub(super) is_real: T,
    pub(super) is_first: T,
    pub(super) is_last: T,
    pub(super) tx_index: T,
    pub(super) instruction_index: T,
    pub(super) capability_transcript_id: T,
    pub(super) input_count: T,
    pub(super) output_count: T,
    pub(super) total_payload_len: T,
    pub(super) chunk_index: T,
    pub(super) chunk_len: T,
    pub(super) prev_digest: [T; 8],
    pub(super) perm_input: [T; 16],
    pub(super) perm_output: [T; 8],
}

/// Generic transcript chip for capability calls.
#[derive(Clone, Copy, Debug, Default)]
pub struct CapabilityTranscriptChip;

impl crate::ChipSpec for CapabilityTranscriptChip {
    fn chip_id(&self) -> ChipId {
        CAPABILITY_TRANSCRIPT_CHIP_ID
    }
}

impl<F> BaseAir<F> for CapabilityTranscriptChip {
    fn width(&self) -> usize {
        CAPABILITY_TRANSCRIPT_WIDTH
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for CapabilityTranscriptChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &CapabilityTranscriptCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &CapabilityTranscriptCols<AB::Var> = borrow_cols(main.next_slice());

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();

        builder.assert_bool(local.is_real);
        builder.assert_bool(local.is_first);
        builder.assert_bool(local.is_last);
        constrain_is_real_prefix(builder, local.is_real, next.is_real);

        // First rows start at chunk zero with no previous digest.
        builder.assert_zero(is_real.clone() * local.is_first.into() * local.chunk_index.into());
        for idx in 0..8 {
            builder.assert_zero(
                is_real.clone() * local.is_first.into() * local.prev_digest[idx].into(),
            );
        }

        // Transitions either continue the same event or start a new one.
        let continue_event: AB::Expr = both_real.clone() * (AB::Expr::ONE - local.is_last.into());
        let next_event: AB::Expr = both_real.clone() * local.is_last.into();

        builder
            .when_transition()
            .assert_zero(continue_event.clone() * next.is_first.into());
        builder
            .when_transition()
            .assert_zero(continue_event.clone() * (next.tx_index.into() - local.tx_index.into()));
        builder.when_transition().assert_zero(
            continue_event.clone()
                * (next.instruction_index.into() - local.instruction_index.into()),
        );
        builder.when_transition().assert_zero(
            continue_event.clone()
                * (next.capability_transcript_id.into() - local.capability_transcript_id.into()),
        );
        builder.when_transition().assert_zero(
            continue_event.clone() * (next.input_count.into() - local.input_count.into()),
        );
        builder.when_transition().assert_zero(
            continue_event.clone() * (next.output_count.into() - local.output_count.into()),
        );
        builder.when_transition().assert_zero(
            continue_event.clone()
                * (next.total_payload_len.into() - local.total_payload_len.into()),
        );
        builder.when_transition().assert_zero(
            continue_event.clone()
                * (next.chunk_index.into() - local.chunk_index.into() - AB::Expr::ONE),
        );
        for idx in 0..8 {
            builder.when_transition().assert_zero(
                continue_event.clone()
                    * (next.prev_digest[idx].into() - local.perm_output[idx].into()),
            );
        }

        builder
            .when_transition()
            .assert_zero(next_event.clone() * (next.is_first.into() - AB::Expr::ONE));

        // Poseidon input wiring.
        let first_gate: AB::Expr = is_real.clone() * local.is_first.into();
        let cont_gate: AB::Expr = is_real.clone() * (AB::Expr::ONE - local.is_first.into());

        builder.assert_zero(
            first_gate.clone()
                * (local.perm_input[0].into()
                    - expr_from_u32::<AB>(CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG)),
        );
        builder
            .assert_zero(first_gate.clone() * (local.perm_input[1].into() - local.tx_index.into()));
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[2].into() - local.instruction_index.into()),
        );
        builder.assert_zero(
            first_gate.clone()
                * (local.perm_input[3].into() - local.capability_transcript_id.into()),
        );
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[4].into() - local.input_count.into()),
        );
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[5].into() - local.output_count.into()),
        );
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[6].into() - local.total_payload_len.into()),
        );
        builder.assert_zero(first_gate * (local.perm_input[7].into() - local.chunk_len.into()));

        builder.assert_zero(
            cont_gate.clone()
                * (local.perm_input[0].into()
                    - expr_from_u32::<AB>(CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG)),
        );
        builder.assert_zero(
            cont_gate.clone() * (local.perm_input[1].into() - local.chunk_index.into()),
        );
        builder
            .assert_zero(cont_gate.clone() * (local.perm_input[2].into() - local.chunk_len.into()));
        for idx in 0..8 {
            builder.assert_zero(
                cont_gate.clone()
                    * (local.perm_input[3 + idx].into() - local.prev_digest[idx].into()),
            );
        }

        let mut poseidon_values = Vec::with_capacity(24);
        for idx in 0..16 {
            poseidon_values.push(local.perm_input[idx].into());
        }
        for idx in 0..8 {
            poseidon_values.push(local.perm_output[idx].into());
        }
        builder.send(AirInteraction {
            values: poseidon_values,
            multiplicity: is_real.clone(),
            bus: core_buses::POSEIDON_PERM,
        });

        let mut header_values = Vec::with_capacity(13);
        header_values.push(local.tx_index.into());
        header_values.push(local.instruction_index.into());
        header_values.push(local.capability_transcript_id.into());
        header_values.push(local.input_count.into());
        header_values.push(local.output_count.into());
        for idx in 0..8 {
            header_values.push(local.perm_output[idx].into());
        }
        let relay_mult: AB::Expr = is_real * local.is_last.into();
        builder.receive(AirInteraction {
            values: header_values.clone(),
            multiplicity: relay_mult.clone(),
            bus: core_buses::CAPABILITY_TRANSCRIPT,
        });
        builder.send(AirInteraction {
            values: header_values,
            multiplicity: relay_mult,
            bus: core_buses::CAPABILITY_TRANSCRIPT,
        });
    }
}
