//! Generic transcript lane for precompile calls.
//!
//! This chip proves a canonical, length-delimited transcript digest for each
//! precompile call, then relays the call header on the shared PRECOMPILE bus.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::KoalaBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{PrecompileEvent, Value, ValueType};
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut};
use tabula_stark::air::interaction::{AirInteraction, core_buses};
use tabula_stark::chips::ChipId;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use crate::poseidon::constants::poseidon2_permutation;

/// Witness-store label for transcript calls consumed by [`PrecompileTranscriptChip`].
pub const PRECOMPILE_TRANSCRIPT_WITNESS_LABEL: &str = "precompile_transcript_calls";

/// Fixed chip id for the generic precompile transcript lane.
pub const PRECOMPILE_TRANSCRIPT_CHIP_ID: ChipId = ChipId(90);

/// Domain tag for the first transcript row of one precompile call.
pub const PRECOMPILE_TRANSCRIPT_FIRST_DOMAIN_TAG: u32 = 0x31;
/// Domain tag for continuation transcript rows of one precompile call.
pub const PRECOMPILE_TRANSCRIPT_CONT_DOMAIN_TAG: u32 = 0x32;

const FIRST_ROW_PAYLOAD_CAPACITY: usize = 8;
const CONT_ROW_PAYLOAD_CAPACITY: usize = 5;
const PRECOMPILE_TRANSCRIPT_WIDTH: usize = 43;

/// Bus-visible header for one precompile call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecompileCallHeader {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Zero-based instruction index in the tx body.
    pub instruction_index: u32,
    /// Precompile identifier.
    pub precompile_id: u16,
    /// Number of input values.
    pub input_count: u32,
    /// Number of output values.
    pub output_count: u32,
    /// Canonical transcript digest, encoded as the first 8 Poseidon outputs.
    pub event_digest: [u32; 8],
}

/// Canonical transcript witness for one precompile call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecompileTranscriptCall {
    /// Original execution event.
    pub event: PrecompileEvent,
    /// Bus-visible header.
    pub header: PrecompileCallHeader,
    /// Canonically encoded payload (typed inputs, then typed outputs).
    pub payload: Vec<KoalaBear>,
}

impl PrecompileTranscriptCall {
    /// Build one canonical transcript witness from a structured execution event.
    pub fn from_event(event: &PrecompileEvent) -> Result<Self, TabulaError> {
        let payload = encode_precompile_event_payload(event)?;
        let header = build_precompile_call_header(event, &payload)?;
        Ok(Self {
            event: event.clone(),
            header,
            payload,
        })
    }
}

/// Compute the canonical call header for one execution event.
pub fn compute_precompile_call_header(
    event: &PrecompileEvent,
) -> Result<PrecompileCallHeader, TabulaError> {
    let payload = encode_precompile_event_payload(event)?;
    build_precompile_call_header(event, &payload)
}

fn build_precompile_call_header(
    event: &PrecompileEvent,
    payload: &[KoalaBear],
) -> Result<PrecompileCallHeader, TabulaError> {
    let tx_index = u32::try_from(event.tx_index).map_err(|_| TabulaError::ProofError {
        phase: "precompile_transcript",
        detail: format!("tx_index {} exceeds u32 range", event.tx_index),
    })?;
    let instruction_index =
        u32::try_from(event.instruction_index).map_err(|_| TabulaError::ProofError {
            phase: "precompile_transcript",
            detail: format!(
                "instruction_index {} exceeds u32 range",
                event.instruction_index
            ),
        })?;

    let digest = compute_event_digest(
        tx_index,
        instruction_index,
        event.precompile_id,
        event,
        payload,
    );
    Ok(PrecompileCallHeader {
        tx_index,
        instruction_index,
        precompile_id: event.precompile_id,
        input_count: event.inputs.len() as u32,
        output_count: event.outputs.len() as u32,
        event_digest: digest,
    })
}

/// Encode one precompile event into a canonical payload over KoalaBear field elements.
pub fn encode_precompile_event_payload(
    event: &PrecompileEvent,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let codec = KoalaBearCodec;
    let mut payload = Vec::new();
    append_value_sequence(&mut payload, &codec, &event.inputs)?;
    append_value_sequence(&mut payload, &codec, &event.outputs)?;
    Ok(payload)
}

fn append_value_sequence(
    payload: &mut Vec<KoalaBear>,
    codec: &KoalaBearCodec,
    values: &[Value],
) -> Result<(), TabulaError> {
    for value in values {
        let encoded = codec.encode(value)?;
        payload.push(KoalaBear::new(value_type_tag(value.value_type())));
        payload.push(KoalaBear::new(encoded.len() as u32));
        payload.extend(encoded);
    }
    Ok(())
}

fn value_type_tag(value_type: ValueType) -> u32 {
    match value_type {
        ValueType::U64 => 1,
        ValueType::I64 => 2,
        ValueType::Bool => 3,
        ValueType::Bytes32 => 4,
    }
}

fn compute_event_digest(
    tx_index: u32,
    instruction_index: u32,
    precompile_id: u16,
    event: &PrecompileEvent,
    payload: &[KoalaBear],
) -> [u32; 8] {
    let total_payload_len = payload.len();
    let mut chunk_index = 0u32;
    let mut offset = 0usize;
    let first_chunk_len = total_payload_len.min(FIRST_ROW_PAYLOAD_CAPACITY);
    let first_input = build_first_row_perm_input(
        tx_index,
        instruction_index,
        precompile_id,
        event.inputs.len() as u32,
        event.outputs.len() as u32,
        total_payload_len as u32,
        &payload[..first_chunk_len],
    );
    let (_, first_output) = poseidon2_permutation(first_input);
    let mut last_output = core::array::from_fn(|idx| first_output[idx]);
    offset += first_chunk_len;
    chunk_index += 1;

    while offset < total_payload_len {
        let chunk_len = (total_payload_len - offset).min(CONT_ROW_PAYLOAD_CAPACITY);
        let input = build_cont_row_perm_input(
            chunk_index,
            last_output,
            &payload[offset..offset + chunk_len],
        );
        let (_, output) = poseidon2_permutation(input);
        last_output = core::array::from_fn(|idx| output[idx]);
        offset += chunk_len;
        chunk_index += 1;
    }

    core::array::from_fn(|idx| last_output[idx].as_canonical_u32())
}

fn build_first_row_perm_input(
    tx_index: u32,
    instruction_index: u32,
    precompile_id: u16,
    input_count: u32,
    output_count: u32,
    total_payload_len: u32,
    payload_chunk: &[KoalaBear],
) -> [KoalaBear; 16] {
    let mut input = [KoalaBear::ZERO; 16];
    input[0] = KoalaBear::new(PRECOMPILE_TRANSCRIPT_FIRST_DOMAIN_TAG);
    input[1] = KoalaBear::new(tx_index);
    input[2] = KoalaBear::new(instruction_index);
    input[3] = KoalaBear::new(precompile_id as u32);
    input[4] = KoalaBear::new(input_count);
    input[5] = KoalaBear::new(output_count);
    input[6] = KoalaBear::new(total_payload_len);
    input[7] = KoalaBear::new(payload_chunk.len() as u32);
    for (idx, value) in payload_chunk.iter().enumerate() {
        input[8 + idx] = *value;
    }
    input
}

fn build_cont_row_perm_input(
    chunk_index: u32,
    prev_digest: [KoalaBear; 8],
    payload_chunk: &[KoalaBear],
) -> [KoalaBear; 16] {
    let mut input = [KoalaBear::ZERO; 16];
    input[0] = KoalaBear::new(PRECOMPILE_TRANSCRIPT_CONT_DOMAIN_TAG);
    input[1] = KoalaBear::new(chunk_index);
    input[2] = KoalaBear::new(payload_chunk.len() as u32);
    input[3..11].copy_from_slice(&prev_digest);
    for (idx, value) in payload_chunk.iter().enumerate() {
        input[11 + idx] = *value;
    }
    input
}

#[repr(C)]
struct PrecompileTranscriptCols<T> {
    is_real: T,
    is_first: T,
    is_last: T,
    tx_index: T,
    instruction_index: T,
    precompile_id: T,
    input_count: T,
    output_count: T,
    total_payload_len: T,
    chunk_index: T,
    chunk_len: T,
    prev_digest: [T; 8],
    perm_input: [T; 16],
    perm_output: [T; 8],
}

#[derive(Clone, Debug)]
struct PrecompileTranscriptRow {
    is_first: bool,
    is_last: bool,
    header: PrecompileCallHeader,
    total_payload_len: u32,
    chunk_index: u32,
    chunk_len: u32,
    prev_digest: [u32; 8],
    perm_input: [KoalaBear; 16],
    perm_output: [u32; 8],
}

/// Generic transcript chip for precompile calls.
#[derive(Clone, Copy, Debug, Default)]
pub struct PrecompileTranscriptChip;

impl crate::ChipSpec for PrecompileTranscriptChip {
    fn chip_id(&self) -> ChipId {
        PRECOMPILE_TRANSCRIPT_CHIP_ID
    }
}

impl<F> BaseAir<F> for PrecompileTranscriptChip {
    fn width(&self) -> usize {
        PRECOMPILE_TRANSCRIPT_WIDTH
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for PrecompileTranscriptChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &PrecompileTranscriptCols<AB::Var> = borrow_cols(main.current_slice());
        let next: &PrecompileTranscriptCols<AB::Var> = borrow_cols(main.next_slice());

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
            continue_event.clone() * (next.precompile_id.into() - local.precompile_id.into()),
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
                    - expr_from_u32::<AB>(PRECOMPILE_TRANSCRIPT_FIRST_DOMAIN_TAG)),
        );
        builder
            .assert_zero(first_gate.clone() * (local.perm_input[1].into() - local.tx_index.into()));
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[2].into() - local.instruction_index.into()),
        );
        builder.assert_zero(
            first_gate.clone() * (local.perm_input[3].into() - local.precompile_id.into()),
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
                    - expr_from_u32::<AB>(PRECOMPILE_TRANSCRIPT_CONT_DOMAIN_TAG)),
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
        header_values.push(local.precompile_id.into());
        header_values.push(local.input_count.into());
        header_values.push(local.output_count.into());
        for idx in 0..8 {
            header_values.push(local.perm_output[idx].into());
        }
        let relay_mult: AB::Expr = is_real * local.is_last.into();
        builder.receive(AirInteraction {
            values: header_values.clone(),
            multiplicity: relay_mult.clone(),
            bus: core_buses::PRECOMPILE,
        });
        builder.send(AirInteraction {
            values: header_values,
            multiplicity: relay_mult,
            bus: core_buses::PRECOMPILE,
        });
    }
}

impl TraceContributor for PrecompileTranscriptChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let calls =
            store.get::<Vec<PrecompileTranscriptCall>>(PRECOMPILE_TRANSCRIPT_WITNESS_LABEL)?;
        let rows = build_transcript_rows(calls);
        let num_real = rows.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * PRECOMPILE_TRANSCRIPT_WIDTH];

        for (row_idx, row) in rows.iter().enumerate() {
            let offset = row_idx * PRECOMPILE_TRANSCRIPT_WIDTH;
            let cols: &mut PrecompileTranscriptCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + PRECOMPILE_TRANSCRIPT_WIDTH]);
            cols.is_real = KoalaBear::ONE;
            cols.is_first = if row.is_first {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.is_last = if row.is_last {
                KoalaBear::ONE
            } else {
                KoalaBear::ZERO
            };
            cols.tx_index = KoalaBear::new(row.header.tx_index);
            cols.instruction_index = KoalaBear::new(row.header.instruction_index);
            cols.precompile_id = KoalaBear::new(row.header.precompile_id as u32);
            cols.input_count = KoalaBear::new(row.header.input_count);
            cols.output_count = KoalaBear::new(row.header.output_count);
            cols.total_payload_len = KoalaBear::new(row.total_payload_len);
            cols.chunk_index = KoalaBear::new(row.chunk_index);
            cols.chunk_len = KoalaBear::new(row.chunk_len);
            for idx in 0..8 {
                cols.prev_digest[idx] = KoalaBear::new(row.prev_digest[idx]);
            }
            cols.perm_input = row.perm_input;
            for idx in 0..8 {
                cols.perm_output[idx] = KoalaBear::new(row.perm_output[idx]);
            }
        }

        map.insert(
            PRECOMPILE_TRANSCRIPT_CHIP_ID,
            RowMajorMatrix::new(values, PRECOMPILE_TRANSCRIPT_WIDTH),
        );
        Ok(())
    }
}

fn build_transcript_rows(calls: &[PrecompileTranscriptCall]) -> Vec<PrecompileTranscriptRow> {
    let mut rows = Vec::new();
    for call in calls {
        let total_payload_len = call.payload.len() as u32;
        let first_chunk_len = call.payload.len().min(FIRST_ROW_PAYLOAD_CAPACITY);
        let mut offset = 0usize;
        let mut chunk_index = 0u32;
        let first_input = build_first_row_perm_input(
            call.header.tx_index,
            call.header.instruction_index,
            call.header.precompile_id,
            call.header.input_count,
            call.header.output_count,
            total_payload_len,
            &call.payload[..first_chunk_len],
        );
        let (_, first_output) = poseidon2_permutation(first_input);
        let mut prev_digest = [0u32; 8];
        let mut current_digest: [u32; 8] =
            core::array::from_fn(|idx| first_output[idx].as_canonical_u32());
        rows.push(PrecompileTranscriptRow {
            is_first: true,
            is_last: first_chunk_len == call.payload.len(),
            header: call.header.clone(),
            total_payload_len,
            chunk_index,
            chunk_len: first_chunk_len as u32,
            prev_digest,
            perm_input: first_input,
            perm_output: current_digest,
        });
        offset += first_chunk_len;
        chunk_index += 1;
        prev_digest = current_digest;

        while offset < call.payload.len() {
            let chunk_len = (call.payload.len() - offset).min(CONT_ROW_PAYLOAD_CAPACITY);
            let mut prev_fe = [KoalaBear::ZERO; 8];
            for idx in 0..8 {
                prev_fe[idx] = KoalaBear::new(prev_digest[idx]);
            }
            let input = build_cont_row_perm_input(
                chunk_index,
                prev_fe,
                &call.payload[offset..offset + chunk_len],
            );
            let (_, output) = poseidon2_permutation(input);
            current_digest = core::array::from_fn(|idx| output[idx].as_canonical_u32());
            rows.push(PrecompileTranscriptRow {
                is_first: false,
                is_last: offset + chunk_len == call.payload.len(),
                header: call.header.clone(),
                total_payload_len,
                chunk_index,
                chunk_len: chunk_len as u32,
                prev_digest,
                perm_input: input,
                perm_output: current_digest,
            });
            prev_digest = current_digest;
            offset += chunk_len;
            chunk_index += 1;
        }
    }
    rows
}
