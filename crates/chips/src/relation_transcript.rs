//! Dedicated transcript lane for relation input/output tuples.
//!
//! This chip proves a fixed field-oriented transcript over typed tuple metadata
//! and padded value limbs. Execution binds the tuple columns; this lane binds
//! the final digest.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_contract::format::typed_tuple::{
    EncodedTypedTupleElement, MaterializedTypedTuple, TYPED_TUPLE_BLOCKS, TYPED_TUPLE_MAX_SLOTS,
    TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG, TYPED_TUPLE_TRANSCRIPT_RATE, TYPED_TUPLE_VALUE_WIDTH,
    TupleEncodingDefaults, TypedTupleRole, materialize_typed_tuple,
};
use tabula_core::error::TabulaError;
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::air::interaction::{AirInteraction, BusId};
use tabula_stark::chips::{ChipId, ChipSpec};
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;
use tabula_types::{EncodingRuntimeRegistry, TypedValue};

use crate::execution::{EXECUTION_STANDARD_VALUE_WIDTH, MAX_SLOTS};
use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::{TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation};
use crate::poseidon::generate_poseidon_preprocessed;

const _: [(); MAX_SLOTS] = [(); TYPED_TUPLE_MAX_SLOTS];
const _: [(); EXECUTION_STANDARD_VALUE_WIDTH] = [(); TYPED_TUPLE_VALUE_WIDTH];

/// Witness-store label for relation tuple transcript calls.
pub const RELATION_TRANSCRIPT_WITNESS_LABEL: &str = "relation_transcript_calls";
/// Chip id for the relation transcript lane.
pub const RELATION_TRANSCRIPT_CHIP_ID: ChipId = ChipId(93);
/// Private bus binding execution tuple values to transcript calls.
pub const RELATION_TUPLE_BUS: BusId = BusId(102);
/// Private bus relaying tuple digests back to execution rows.
pub const RELATION_DIGEST_BUS: BusId = BusId(103);

/// Canonical transcript witness for one relation tuple digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTranscriptCall {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Effect ordinal within the transaction; this is the canonical tuple/digest relay key.
    pub effect_ordinal_in_tx: u32,
    /// Zero-based canonical op index.
    pub instruction_index: u32,
    /// Tuple role.
    pub role: TypedTupleRole,
    /// Prefix occupancy in tuple order.
    pub tuple_used: [bool; TYPED_TUPLE_MAX_SLOTS],
    /// Type id per tuple position.
    pub tuple_type_ids: [u32; TYPED_TUPLE_MAX_SLOTS],
    /// Padded field-element encodings per tuple position.
    pub tuple_values: [[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    /// Fixed transcript blocks absorbed by this call.
    pub blocks: [[KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE]; TYPED_TUPLE_BLOCKS],
    /// Final digest as the first eight KoalaBear words of the terminal sponge state.
    pub digest: [u32; 8],
}

impl RelationTranscriptCall {
    /// Build one relation transcript call from concrete typed tuple values.
    pub fn from_typed_values(
        tx_index: u32,
        effect_ordinal_in_tx: u32,
        instruction_index: u32,
        role: TypedTupleRole,
        values: &[TypedValue],
        tuple_encoding_defaults: &TupleEncodingDefaults,
        encoding_runtimes: &EncodingRuntimeRegistry,
    ) -> Result<Self, TabulaError> {
        let encoded_values =
            encode_tuple_elements(values, tuple_encoding_defaults, encoding_runtimes)?;
        let materialized = materialize_typed_tuple(role, &encoded_values)?;
        Ok(Self::from_materialized(
            tx_index,
            effect_ordinal_in_tx,
            instruction_index,
            &materialized,
        ))
    }

    fn from_materialized(
        tx_index: u32,
        effect_ordinal_in_tx: u32,
        instruction_index: u32,
        materialized: &MaterializedTypedTuple,
    ) -> Self {
        Self {
            tx_index,
            effect_ordinal_in_tx,
            instruction_index,
            role: materialized.role,
            tuple_used: materialized.used,
            tuple_type_ids: materialized.type_ids,
            tuple_values: materialized.values,
            blocks: materialized.blocks,
            digest: materialized.digest,
        }
    }
}

fn encode_tuple_elements(
    values: &[TypedValue],
    tuple_encoding_defaults: &TupleEncodingDefaults,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<Vec<EncodedTypedTupleElement>, TabulaError> {
    values
        .iter()
        .map(|value| {
            let encoding_profile_id = tuple_encoding_defaults.resolve(value.type_id())?;
            Ok(EncodedTypedTupleElement {
                type_id: value.type_id(),
                field_elements: encoding_runtimes
                    .encode_field_elements_for_profile(encoding_profile_id, value)?,
            })
        })
        .collect()
}

#[repr(C)]
struct RelationTranscriptCols<T> {
    tx_index: T,
    effect_ordinal_in_tx: T,
    tuple_role: T,
    tuple_used: [T; TYPED_TUPLE_MAX_SLOTS],
    tuple_type_ids: [T; TYPED_TUPLE_MAX_SLOTS],
    tuple_values: [[T; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    block_sel: [T; TYPED_TUPLE_BLOCKS],
    block_values: [T; TYPED_TUPLE_TRANSCRIPT_RATE],
    prev_digest: [T; 8],
    perm_state_out: [T; WIDTH],
    poseidon: PoseidonCols<T>,
}

const fn relation_transcript_width() -> usize {
    num_cols::<RelationTranscriptCols<u8>, u8>()
}

#[derive(Clone, Debug)]
struct RelationTranscriptRoundRow {
    tx_index: u32,
    effect_ordinal_in_tx: u32,
    tuple_role: TypedTupleRole,
    tuple_used: [bool; TYPED_TUPLE_MAX_SLOTS],
    tuple_type_ids: [u32; TYPED_TUPLE_MAX_SLOTS],
    tuple_values: [[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    block_index: usize,
    block_values: [KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE],
    prev_digest: [u32; 8],
    perm_state_out: [KoalaBear; WIDTH],
    round_ctr: u32,
    round_data: crate::poseidon::constants::PoseidonRoundData,
    perm_input: [KoalaBear; WIDTH],
    perm_output: [KoalaBear; 8],
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

impl TraceGenerator for RelationTranscriptChip {
    type Input = [RelationTranscriptCall];

    fn generate_trace(&self, input: &[RelationTranscriptCall]) -> RowMajorMatrix<KoalaBear> {
        let width = relation_transcript_width();
        let rows = build_round_rows(input);
        let num_real = rows.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * width];

        for (row_idx, row) in rows.iter().enumerate() {
            let offset = row_idx * width;
            let cols: &mut RelationTranscriptCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + width]);
            cols.tx_index = KoalaBear::new(row.tx_index);
            cols.effect_ordinal_in_tx = KoalaBear::new(row.effect_ordinal_in_tx);
            cols.tuple_role = KoalaBear::new(row.tuple_role.as_u32());
            for idx in 0..MAX_SLOTS {
                cols.tuple_used[idx] = if row.tuple_used[idx] {
                    KoalaBear::ONE
                } else {
                    KoalaBear::ZERO
                };
                cols.tuple_type_ids[idx] = KoalaBear::new(row.tuple_type_ids[idx]);
                cols.tuple_values[idx] = row.tuple_values[idx];
            }
            cols.block_sel[row.block_index] = KoalaBear::ONE;
            cols.block_values = row.block_values;
            for idx in 0..8 {
                cols.prev_digest[idx] = KoalaBear::new(row.prev_digest[idx]);
            }
            cols.perm_state_out = row.perm_state_out;
            cols.poseidon.is_real = KoalaBear::ONE;
            cols.poseidon.is_first_round = if row.round_ctr == 0 {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.poseidon.is_last_round = if row.round_ctr + 1 == TOTAL_ROUNDS as u32 {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.poseidon.is_full_round = if is_full_round(row.round_ctr as usize) {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.poseidon.round_ctr = KoalaBear::new(row.round_ctr);
            cols.poseidon.perm_input = row.perm_input;
            cols.poseidon.rc = row.round_data.rc;
            cols.poseidon.state = row.round_data.state_before;
            cols.poseidon.sbox_y2 = row.round_data.sbox_y2;
            cols.poseidon.sbox_y3 = row.round_data.sbox_y3;
            cols.poseidon.perm_output = row.perm_output;
        }

        RowMajorMatrix::new(values, width)
    }
}

impl TraceContributor for RelationTranscriptChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let calls = store.get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)?;
        map.insert_with_preprocessed(
            RELATION_TRANSCRIPT_CHIP_ID,
            self.generate_trace(calls.as_slice()),
            generate_poseidon_preprocessed(calls.len() * TYPED_TUPLE_BLOCKS),
        );
        Ok(())
    }
}

fn build_round_rows(calls: &[RelationTranscriptCall]) -> Vec<RelationTranscriptRoundRow> {
    let mut rows = Vec::new();
    for call in calls {
        let mut prev_digest = [0u32; 8];
        for (block_index, block_values) in call.blocks.iter().enumerate() {
            let mut perm_input = [KoalaBear::ZERO; WIDTH];
            for idx in 0..8 {
                perm_input[idx] = KoalaBear::new(prev_digest[idx]);
            }
            perm_input[8..(8 + TYPED_TUPLE_TRANSCRIPT_RATE)]
                .copy_from_slice(&block_values[..TYPED_TUPLE_TRANSCRIPT_RATE]);
            let (rounds, output_state) = poseidon2_permutation(perm_input);
            let current_digest = core::array::from_fn(|idx| output_state[idx].as_canonical_u32());

            for (round_ctr, round_data) in rounds.into_iter().enumerate() {
                rows.push(RelationTranscriptRoundRow {
                    tx_index: call.tx_index,
                    effect_ordinal_in_tx: call.effect_ordinal_in_tx,
                    tuple_role: call.role,
                    tuple_used: call.tuple_used,
                    tuple_type_ids: call.tuple_type_ids,
                    tuple_values: call.tuple_values,
                    block_index,
                    block_values: *block_values,
                    prev_digest,
                    perm_state_out: output_state,
                    round_ctr: round_ctr as u32,
                    round_data,
                    perm_input,
                    perm_output: core::array::from_fn(|idx| output_state[idx]),
                });
            }
            prev_digest = current_digest;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use tabula_contract::format::typed_tuple::{TupleEncodingDefaults, TupleEncodingSelection};
    use tabula_core::EncodingProfileId;
    use tabula_core::error::TabulaError;
    use tabula_profile::{
        CanonicalNullEncoding, ENCODING_U64_ID, EncodingClass, EncodingProfile, FieldFamily,
        TYPE_U64_ID, TranscriptSerialization, builtin_catalog,
    };
    use tabula_stark::debug::debug_check;
    use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, u64_typed};

    const ALT_U64_ENCODING_ID: EncodingProfileId = EncodingProfileId(0x8000_c301);

    #[derive(Clone)]
    struct AltU64EncodingRuntime {
        descriptor: EncodingProfile,
        builtin: Arc<dyn EncodingRuntime>,
    }

    impl AltU64EncodingRuntime {
        fn new() -> Self {
            let catalog = builtin_catalog().expect("built-in catalog");
            let descriptor = catalog
                .type_descriptor(TYPE_U64_ID)
                .expect("u64 descriptor")
                .clone();
            Self {
                descriptor: EncodingProfile::new(
                    ALT_U64_ENCODING_ID,
                    "u64_kb3_alt",
                    None,
                    &descriptor,
                    EncodingClass::FieldElementArray,
                    FieldFamily::KoalaBear31,
                    3,
                    CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                    TranscriptSerialization::FieldElementsWithNullFlag,
                    true,
                )
                .expect("alt u64 encoding"),
                builtin: EncodingRuntimeRegistry::seeded()
                    .expect("seeded encoding runtimes")
                    .resolve(ENCODING_U64_ID)
                    .expect("builtin u64 encoding")
                    .clone(),
            }
        }
    }

    impl EncodingRuntime for AltU64EncodingRuntime {
        fn encoding_profile_id(&self) -> EncodingProfileId {
            self.descriptor.encoding_profile_id
        }

        fn descriptor(&self) -> &EncodingProfile {
            &self.descriptor
        }

        fn encode_field_elements(
            &self,
            value: &tabula_types::TypedValue,
        ) -> Result<Vec<KoalaBear>, TabulaError> {
            self.builtin.encode_field_elements(value)
        }

        fn decode_field_elements(
            &self,
            field_elements: &[KoalaBear],
        ) -> Result<tabula_types::TypedValue, TabulaError> {
            self.builtin.decode_field_elements(field_elements)
        }

        fn encode_transcript_atoms(
            &self,
            value: &tabula_types::TypedValue,
        ) -> Result<Vec<KoalaBear>, TabulaError> {
            self.builtin.encode_transcript_atoms(value)
        }

        fn trace_width(&self) -> usize {
            self.descriptor.width as usize
        }
    }

    fn tuple_encoding_defaults() -> TupleEncodingDefaults {
        TupleEncodingDefaults::new(vec![TupleEncodingSelection {
            type_id: TYPE_U64_ID,
            encoding_profile_id: ENCODING_U64_ID,
        }])
        .expect("tuple defaults")
    }

    fn call(
        tx_index: u32,
        effect_ordinal_in_tx: u32,
        instruction_index: u32,
        role: TypedTupleRole,
        values: &[TypedValue],
    ) -> RelationTranscriptCall {
        let encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        let tuple_encoding_defaults = tuple_encoding_defaults();
        RelationTranscriptCall::from_typed_values(
            tx_index,
            effect_ordinal_in_tx,
            instruction_index,
            role,
            values,
            &tuple_encoding_defaults,
            &encoding_runtimes,
        )
        .expect("build relation transcript call")
    }

    #[test]
    fn transcript_real_gap_attack_fails() {
        let chip = RelationTranscriptChip;
        let mut trace = chip.generate_trace(&[call(
            0,
            0,
            7,
            TypedTupleRole::RelationInput,
            &[u64_typed(9)],
        )]);
        let gap_row = TOTAL_ROUNDS;
        let width = relation_transcript_width();
        let row = &mut trace.values[gap_row * width..(gap_row + 1) * width];
        let cols: &mut RelationTranscriptCols<KoalaBear> = borrow_cols_mut(row);
        cols.poseidon.is_real = KoalaBear::ZERO;

        debug_check(&chip, &trace).expect_err("real-row gaps must fail");
    }

    #[test]
    fn transcript_cross_call_carry_attack_fails() {
        let chip = RelationTranscriptChip;
        let first = call(0, 0, 7, TypedTupleRole::RelationInput, &[u64_typed(1)]);
        let second = call(0, 1, 8, TypedTupleRole::RelationInput, &[u64_typed(2)]);
        let mut trace = chip.generate_trace(&[first, second]);
        let second_call_first_row = TYPED_TUPLE_BLOCKS * TOTAL_ROUNDS;
        let width = relation_transcript_width();
        let row =
            &mut trace.values[second_call_first_row * width..(second_call_first_row + 1) * width];
        let cols: &mut RelationTranscriptCols<KoalaBear> = borrow_cols_mut(row);
        cols.prev_digest[0] = KoalaBear::ONE;

        debug_check(&chip, &trace).expect_err("cross-call carry must fail");
    }

    #[test]
    fn transcript_call_uses_sealed_encoding_selection_without_type_ambiguity() {
        let mut encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        encoding_runtimes
            .register(Arc::new(AltU64EncodingRuntime::new()))
            .expect("register alt encoding");
        let tuple_encoding_defaults = TupleEncodingDefaults::new(vec![TupleEncodingSelection {
            type_id: TYPE_U64_ID,
            encoding_profile_id: ALT_U64_ENCODING_ID,
        }])
        .expect("tuple defaults");

        let call = RelationTranscriptCall::from_typed_values(
            0,
            0,
            7,
            TypedTupleRole::RelationInput,
            &[u64_typed(9)],
            &tuple_encoding_defaults,
            &encoding_runtimes,
        )
        .expect("build relation transcript call with explicit encoding");

        assert!(call.tuple_used[0]);
        assert!(!call.tuple_used[1]);
    }
}
