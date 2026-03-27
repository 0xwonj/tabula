//! Witness trace generation for relation transcript calls.
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_contract::format::typed_tuple::{TYPED_TUPLE_BLOCKS, TYPED_TUPLE_TRANSCRIPT_RATE};
use tabula_core::error::TabulaError;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use crate::execution::MAX_SLOTS;
use crate::poseidon::constants::{TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation};
use crate::poseidon::generate_poseidon_preprocessed;

use super::air::{
    RelationTranscriptChip, RelationTranscriptCols, RelationTranscriptRoundRow,
    relation_transcript_width,
};
use super::call::{
    RELATION_TRANSCRIPT_CHIP_ID, RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
};

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
    use p3_field::PrimeCharacteristicRing;
    use std::sync::Arc;
    use tabula_stark::air::columns::borrow_cols_mut;

    use tabula_contract::format::typed_tuple::{
        TYPED_TUPLE_BLOCKS, TupleEncodingDefaults, TupleEncodingSelection, TypedTupleRole,
    };
    use tabula_core::EncodingProfileId;
    use tabula_core::error::TabulaError;
    use tabula_profile::{
        CanonicalNullEncoding, ENCODING_U64_ID, EncodingClass, EncodingProfile, FieldFamily,
        TYPE_U64_ID, TranscriptSerialization, builtin_catalog,
    };
    use tabula_stark::debug::debug_check;
    use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypedValue, u64_typed};

    use crate::poseidon::constants::TOTAL_ROUNDS;

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
