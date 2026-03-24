//! Dedicated proof lane for canonical IR hash semantics.
//!
//! This chip proves the portable byte-level `hash_ir` contract used by runtime
//! execution. It models the exact overwrite-mode Poseidon sponge over KoalaBear
//! bytes and relays the final digest back to the execution lane over a private
//! hash bus.

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

/// Witness-store label for canonical IR hash calls.
pub const IR_HASH_WITNESS_LABEL: &str = "ir_hash_calls";

/// Dedicated chip id for the canonical IR hash lane.
pub const IR_HASH_CHIP_ID: ChipId = ChipId(91);

/// Private execution-tier bus used to relay hash digests from execution rows to the IR hash lane.
pub const IR_HASH_BUS: BusId = BusId(100);

const IR_HASH_RATE: usize = 8;

/// Witness record for one canonical `hash_ir` instruction evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrHashCall {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Zero-based instruction index in the tx body.
    pub instruction_index: u32,
    /// Canonically encoded payload bytes for `hash_ir`.
    pub payload: Vec<u8>,
    /// Final digest as the first eight KoalaBear elements of the terminal sponge state.
    pub digest: [u32; 8],
}

impl IrHashCall {
    /// Build one canonical IR hash witness call from already-portable inputs.
    pub fn from_inputs(
        tx_index: u32,
        instruction_index: u32,
        inputs: &[PortableValue],
    ) -> Result<Self, TabulaError> {
        let payload = encode_ir_hash_payload(inputs);
        let digest_bytes = PoseidonHasher::new().hash(&payload);
        let digest = NativeDigest::from_bytes(&digest_bytes)?.0;
        Ok(Self {
            tx_index,
            instruction_index,
            payload,
            digest: core::array::from_fn(|idx| digest[idx].as_canonical_u32()),
        })
    }
}

/// Canonical byte encoding for `hash_ir`.
#[must_use]
pub fn encode_ir_hash_payload(inputs: &[PortableValue]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(DOMAIN_TAG_HASH_IR);
    bytes.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
    for value in inputs {
        bytes.extend_from_slice(&value.type_id().0.to_le_bytes());
        bytes.extend_from_slice(&(value.payload().len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.payload());
    }
    bytes
}

#[repr(C)]
struct IrHashCols<T> {
    tx_index: T,
    instruction_index: T,
    is_first_block: T,
    is_last_block: T,
    block_byte_real: [T; IR_HASH_RATE],
    block_bytes: [T; IR_HASH_RATE],
    perm_state_out: [T; WIDTH],
    poseidon: PoseidonCols<T>,
}

const fn ir_hash_width() -> usize {
    num_cols::<IrHashCols<u8>, u8>()
}

#[derive(Clone, Debug)]
struct IrHashRoundRow {
    tx_index: u32,
    instruction_index: u32,
    is_first_block: bool,
    is_last_block: bool,
    block_byte_real: [bool; IR_HASH_RATE],
    block_bytes: [u8; IR_HASH_RATE],
    perm_state_out: [KoalaBear; WIDTH],
    round_ctr: u32,
    round_data: crate::poseidon::constants::PoseidonRoundData,
    perm_input: [KoalaBear; WIDTH],
    perm_output: [KoalaBear; 8],
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

fn payload_to_field_bytes(payload: &[u8]) -> Vec<KoalaBear> {
    payload
        .iter()
        .map(|byte| KoalaBear::new(*byte as u32))
        .collect()
}
