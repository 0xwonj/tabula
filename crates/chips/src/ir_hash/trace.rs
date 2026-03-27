//! Witness trace generation for the IR-hash transcript family.
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

use super::air::{IrHashChip, IrHashCols, IrHashRoundRow, ir_hash_width};
use super::call::{
    IR_HASH_CHIP_ID, IR_HASH_RATE, IR_HASH_WITNESS_LABEL, IrHashCall, payload_to_field_bytes,
};

impl TraceGenerator for IrHashChip {
    type Input = [IrHashCall];

    fn generate_trace(&self, input: &[IrHashCall]) -> RowMajorMatrix<KoalaBear> {
        let rows = build_hash_rows(input).expect("IR hash witness rows must be constructible");
        let width = ir_hash_width();
        let num_real = rows.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * width];

        for (row_idx, row) in rows.iter().enumerate() {
            let offset = row_idx * width;
            let cols: &mut IrHashCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + width]);
            cols.tx_index = KoalaBear::new(row.tx_index);
            cols.instruction_index = KoalaBear::new(row.instruction_index);
            cols.is_first_block = if row.is_first_block {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.is_last_block = if row.is_last_block {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            for idx in 0..IR_HASH_RATE {
                cols.block_byte_real[idx] = if row.block_byte_real[idx] {
                    KoalaBear::ONE
                } else {
                    KoalaBear::ZERO
                };
                cols.block_bytes[idx] = KoalaBear::new(row.block_bytes[idx] as u32);
            }
            cols.perm_state_out = row.perm_state_out;
            cols.poseidon.state = row.round_data.state_before;
            cols.poseidon.rc = row.round_data.rc;
            cols.poseidon.sbox_y2 = row.round_data.sbox_y2;
            cols.poseidon.sbox_y3 = row.round_data.sbox_y3;
            cols.poseidon.round_ctr = KoalaBear::new(row.round_ctr);
            cols.poseidon.is_full_round =
                if crate::poseidon::constants::is_full_round(row.round_ctr as usize) {
                    KoalaBear::ONE
                } else {
                    KoalaBear::ZERO
                };
            cols.poseidon.is_first_round = if row.round_ctr == 0 {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.poseidon.is_last_round = if row.round_ctr as usize == TOTAL_ROUNDS - 1 {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.poseidon.is_real = KoalaBear::ONE;
            cols.poseidon.perm_input = row.perm_input;
            cols.poseidon.perm_output = row.perm_output;
        }

        RowMajorMatrix::new(values, width)
    }

    fn generate_preprocessed(&self, input: &[IrHashCall]) -> Option<RowMajorMatrix<KoalaBear>> {
        let num_perms = input
            .iter()
            .map(|call| {
                payload_to_field_bytes(&call.payload)
                    .chunks(IR_HASH_RATE)
                    .len()
            })
            .sum();
        Some(generate_poseidon_preprocessed(num_perms))
    }
}

impl TraceContributor for IrHashChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let calls = store.get::<Vec<IrHashCall>>(IR_HASH_WITNESS_LABEL)?;
        map.insert_entry(self.chip_id(), self.build_entry(calls.as_slice()));
        Ok(())
    }
}

fn build_hash_rows(calls: &[IrHashCall]) -> Result<Vec<IrHashRoundRow>, TabulaError> {
    let mut rows = Vec::new();
    let hasher = PoseidonHasher::new();

    for call in calls {
        let payload = payload_to_field_bytes(&call.payload);
        let mut prev_state = [KoalaBear::ZERO; WIDTH];
        let chunks: Vec<&[KoalaBear]> = payload.chunks(IR_HASH_RATE).collect();
        for (block_idx, chunk) in chunks.iter().enumerate() {
            let is_first_block = block_idx == 0;
            let is_last_block = block_idx + 1 == chunks.len();
            let mut byte_real = [false; IR_HASH_RATE];
            let mut block_bytes = [0u8; IR_HASH_RATE];
            let mut perm_input = prev_state;
            for (idx, value) in chunk.iter().enumerate() {
                byte_real[idx] = true;
                block_bytes[idx] = value.as_canonical_u32() as u8;
                perm_input[idx] = *value;
            }

            let (rounds, output_state) = poseidon2_permutation(perm_input);
            let perm_output = core::array::from_fn(|idx| output_state[idx]);
            for (round_ctr, round_data) in rounds.into_iter().enumerate() {
                rows.push(IrHashRoundRow {
                    tx_index: call.tx_index,
                    instruction_index: call.instruction_index,
                    is_first_block,
                    is_last_block,
                    block_byte_real: byte_real,
                    block_bytes,
                    perm_state_out: output_state,
                    round_ctr: round_ctr as u32,
                    round_data,
                    perm_input,
                    perm_output,
                });
            }
            prev_state = output_state;
        }

        let expected = hasher.hash(&call.payload);
        let expected_digest = NativeDigest::from_bytes(&expected)?.0;
        let actual_digest: [u32; 8] =
            core::array::from_fn(|idx| prev_state[idx].as_canonical_u32());
        let expected_words: [u32; 8] =
            core::array::from_fn(|idx| expected_digest[idx].as_canonical_u32());
        if actual_digest != call.digest || actual_digest != expected_words {
            return Err(TabulaError::ProofError {
                phase: "ir_hash",
                detail: format!(
                    "IR hash witness digest mismatch for tx={} instruction={}",
                    call.tx_index, call.instruction_index
                ),
            });
        }
    }

    Ok(rows)
}
