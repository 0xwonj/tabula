//! Generic transcript lane for precompile calls.
//!
//! This chip proves a canonical, length-delimited transcript digest for each
//! precompile call, then relays the call header on the shared PRECOMPILE bus.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_core::error::TabulaError;
use tabula_core::{PortableValue, PrecompileEvent};
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_ir::{PrecompileSignature, PrecompileValueProfile};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut};
use tabula_stark::air::interaction::{AirInteraction, core_buses};
use tabula_stark::chips::ChipId;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

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
    ///
    /// Each value is prefixed by:
    /// `type_id_le32 || encoding_profile_id_le32 || atom_count_le32`,
    /// with every byte represented as one KoalaBear field element.
    pub payload: Vec<KoalaBear>,
}

impl PrecompileTranscriptCall {
    /// Build one canonical transcript witness from a structured execution event.
    pub fn from_event(
        event: &PrecompileEvent,
        expected_precompile_id: u16,
        signature: &PrecompileSignature,
        type_runtimes: &TypeRuntimeRegistry,
        encoding_runtimes: &EncodingRuntimeRegistry,
    ) -> Result<Self, TabulaError> {
        let (header, payload) = materialize_precompile_call_parts(
            event,
            expected_precompile_id,
            signature,
            type_runtimes,
            encoding_runtimes,
        )?;
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
    expected_precompile_id: u16,
    signature: &PrecompileSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<PrecompileCallHeader, TabulaError> {
    materialize_precompile_call_parts(
        event,
        expected_precompile_id,
        signature,
        type_runtimes,
        encoding_runtimes,
    )
    .map(|(header, _)| header)
}

fn materialize_precompile_call_parts(
    event: &PrecompileEvent,
    expected_precompile_id: u16,
    signature: &PrecompileSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(PrecompileCallHeader, Vec<KoalaBear>), TabulaError> {
    let payload = encode_precompile_event_payload(
        event,
        expected_precompile_id,
        signature,
        type_runtimes,
        encoding_runtimes,
    )?;
    let header = build_precompile_call_header(event, expected_precompile_id, signature, &payload)?;
    Ok((header, payload))
}

fn build_precompile_call_header(
    event: &PrecompileEvent,
    expected_precompile_id: u16,
    signature: &PrecompileSignature,
    payload: &[KoalaBear],
) -> Result<PrecompileCallHeader, TabulaError> {
    validate_event_shape(event, expected_precompile_id, signature)?;
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
///
/// Each typed value contributes:
/// `type_id_le32 || encoding_profile_id_le32 || atom_count_le32 || transcript_atoms`.
pub fn encode_precompile_event_payload(
    event: &PrecompileEvent,
    expected_precompile_id: u16,
    signature: &PrecompileSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<Vec<KoalaBear>, TabulaError> {
    validate_event_shape(event, expected_precompile_id, signature)?;
    let mut payload = Vec::new();
    append_value_sequence(
        &mut payload,
        &event.inputs,
        &signature.inputs,
        type_runtimes,
        encoding_runtimes,
    )?;
    append_value_sequence(
        &mut payload,
        &event.outputs,
        &signature.outputs,
        type_runtimes,
        encoding_runtimes,
    )?;
    Ok(payload)
}

fn append_value_sequence(
    payload: &mut Vec<KoalaBear>,
    values: &[PortableValue],
    profiles: &[PrecompileValueProfile],
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(), TabulaError> {
    for (idx, (value, profile)) in values.iter().zip(profiles).enumerate() {
        if value.type_id() != profile.type_id {
            return Err(TabulaError::ProofError {
                phase: "precompile_transcript",
                detail: format!(
                    "precompile value {} declares type {} but event carries type {}",
                    idx,
                    profile.type_id.0,
                    value.type_id().0,
                ),
            });
        }
        let typed = type_runtimes
            .resolve(profile.type_id)?
            .decode_portable(value)?;
        let encoding = encoding_runtimes.resolve(profile.encoding_profile_id)?;
        if encoding.descriptor().type_id != profile.type_id {
            return Err(TabulaError::ProofError {
                phase: "precompile_transcript",
                detail: format!(
                    "encoding profile {} is incompatible with type {} for precompile value {}",
                    profile.encoding_profile_id.0, profile.type_id.0, idx,
                ),
            });
        }
        let atoms = encoding.encode_transcript_atoms(&typed)?;
        append_le32_bytes(payload, profile.type_id.0);
        append_le32_bytes(payload, profile.encoding_profile_id.0);
        append_le32_bytes(
            payload,
            u32::try_from(atoms.len()).map_err(|_| TabulaError::ProofError {
                phase: "precompile_transcript",
                detail: format!(
                    "precompile value {} transcript atom length {} exceeds u32 range",
                    idx,
                    atoms.len(),
                ),
            })?,
        );
        payload.extend(atoms);
    }
    Ok(())
}

fn append_le32_bytes(payload: &mut Vec<KoalaBear>, value: u32) {
    for byte in value.to_le_bytes() {
        payload.push(KoalaBear::new(u32::from(byte)));
    }
}

fn validate_event_shape(
    event: &PrecompileEvent,
    expected_precompile_id: u16,
    signature: &PrecompileSignature,
) -> Result<(), TabulaError> {
    if event.precompile_id != expected_precompile_id {
        return Err(TabulaError::ProofError {
            phase: "precompile_transcript",
            detail: format!(
                "precompile event id 0x{:04x} does not match expected id 0x{:04x}",
                event.precompile_id, expected_precompile_id,
            ),
        });
    }
    if event.inputs.len() != signature.inputs.len() {
        return Err(TabulaError::ProofError {
            phase: "precompile_transcript",
            detail: format!(
                "precompile 0x{:04x} expects {} inputs but event stores {}",
                event.precompile_id,
                signature.inputs.len(),
                event.inputs.len(),
            ),
        });
    }
    if event.outputs.len() != signature.outputs.len() {
        return Err(TabulaError::ProofError {
            phase: "precompile_transcript",
            detail: format!(
                "precompile 0x{:04x} expects {} outputs but event stores {}",
                event.precompile_id,
                signature.outputs.len(),
                event.outputs.len(),
            ),
        });
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use p3_koala_bear::KoalaBear;
    use tabula_core::PrecompileEvent;
    use tabula_core::error::TabulaError;
    use tabula_core::{EncodingProfileId, PortableValue, TypeId};
    use tabula_ir::{PrecompileSignature, PrecompileValueProfile};
    use tabula_profile::{
        CanonicalNullEncoding, ENCODING_U64_ID, EncodingClass, EncodingProfile, FieldFamily,
        GenericIrFamily, HostValueFamily, NullSemantics, TYPE_U64_ID, TranscriptSerialization,
        TypeCapabilities, TypeDescriptor, ZeroValueSpec, builtin_catalog,
    };
    use tabula_types::{
        ArithmeticOp, EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry,
        TypedValue, bool_portable, u64_portable,
    };

    use super::{PrecompileTranscriptCall, compute_precompile_call_header};

    const ALT_U64_ENCODING_ID: EncodingProfileId = EncodingProfileId(0xc301);
    const HIGH_TYPE_ID_A: TypeId = TypeId(0x8000_0001);
    const HIGH_TYPE_ID_B: TypeId = TypeId(0x9000_0001);
    const SHARED_HIGH_TYPE_ENCODING_ID: EncodingProfileId = EncodingProfileId(0x7000_0001);
    const HIGH_ENCODING_ID_A: EncodingProfileId = EncodingProfileId(0x8000_c301);
    const HIGH_ENCODING_ID_B: EncodingProfileId = EncodingProfileId(0x9000_c301);

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

    #[derive(Clone)]
    struct MirroredU64TypeRuntime {
        descriptor: TypeDescriptor,
        builtin: Arc<dyn TypeRuntime>,
    }

    impl MirroredU64TypeRuntime {
        fn new(type_id: TypeId, label: &str) -> Self {
            let builtin = TypeRuntimeRegistry::seeded()
                .expect("seeded type runtimes")
                .resolve(TYPE_U64_ID)
                .expect("builtin u64 runtime")
                .clone();
            Self {
                descriptor: TypeDescriptor::new(
                    type_id,
                    label,
                    None,
                    HostValueFamily::UnsignedInt { bits: 64 },
                    GenericIrFamily::UnsignedInteger,
                    TypeCapabilities {
                        equality: true,
                        ordering: true,
                        arithmetic: true,
                    },
                    ZeroValueSpec::IntegerZero,
                    NullSemantics::NullableWithCanonicalZero,
                )
                .expect("mirrored u64 descriptor"),
                builtin,
            }
        }

        fn to_builtin(value: &TypedValue) -> TypedValue {
            TypedValue::new(TYPE_U64_ID, value.payload().to_vec())
        }

        fn rewrap_builtin(&self, value: &TypedValue) -> TypedValue {
            TypedValue::new(self.descriptor.type_id, value.payload().to_vec())
        }
    }

    impl TypeRuntime for MirroredU64TypeRuntime {
        fn type_id(&self) -> TypeId {
            self.descriptor.type_id
        }

        fn descriptor(&self) -> &TypeDescriptor {
            &self.descriptor
        }

        fn zero_typed(&self) -> TypedValue {
            self.rewrap_builtin(&self.builtin.zero_typed())
        }

        fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
            self.validate(value)?;
            Ok(PortableValue::new(self.type_id(), value.payload().to_vec()))
        }

        fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
            if value.type_id() != self.type_id() {
                return Err(TabulaError::TypeMismatch {
                    expected: format!("type {}", self.type_id().0),
                    actual: format!("type {}", value.type_id().0),
                });
            }
            let builtin = TypedValue::new(TYPE_U64_ID, value.payload().to_vec());
            self.builtin.validate(&builtin)?;
            Ok(self.rewrap_builtin(&builtin))
        }

        fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
            if value.type_id() != self.type_id() {
                return Err(TabulaError::TypeMismatch {
                    expected: format!("type {}", self.type_id().0),
                    actual: format!("type {}", value.type_id().0),
                });
            }
            self.builtin.validate(&Self::to_builtin(value))
        }

        fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
            self.builtin
                .eq_value(&Self::to_builtin(lhs), &Self::to_builtin(rhs))
        }

        fn cmp_value(
            &self,
            lhs: &TypedValue,
            rhs: &TypedValue,
        ) -> Result<std::cmp::Ordering, TabulaError> {
            self.builtin
                .cmp_value(&Self::to_builtin(lhs), &Self::to_builtin(rhs))
        }

        fn apply_arithmetic(
            &self,
            op: ArithmeticOp,
            lhs: &TypedValue,
            rhs: &TypedValue,
        ) -> Result<TypedValue, TabulaError> {
            self.builtin
                .apply_arithmetic(op, &Self::to_builtin(lhs), &Self::to_builtin(rhs))
                .map(|value| self.rewrap_builtin(&value))
        }

        fn divmod(
            &self,
            lhs: &TypedValue,
            rhs: &TypedValue,
        ) -> Result<(TypedValue, TypedValue), TabulaError> {
            self.builtin
                .divmod(&Self::to_builtin(lhs), &Self::to_builtin(rhs))
                .map(|(lhs, rhs)| (self.rewrap_builtin(&lhs), self.rewrap_builtin(&rhs)))
        }

        fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
            self.builtin.debug_display(&Self::to_builtin(value))
        }
    }

    #[derive(Clone)]
    struct MirroredU64EncodingRuntime {
        descriptor: EncodingProfile,
        builtin: Arc<dyn EncodingRuntime>,
    }

    impl MirroredU64EncodingRuntime {
        fn new(type_id: TypeId, encoding_profile_id: EncodingProfileId, label: &str) -> Self {
            let descriptor = MirroredU64TypeRuntime::new(type_id, label).descriptor;
            Self {
                descriptor: EncodingProfile::new(
                    encoding_profile_id,
                    label,
                    None,
                    &descriptor,
                    EncodingClass::FieldElementArray,
                    FieldFamily::KoalaBear31,
                    3,
                    CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                    TranscriptSerialization::FieldElementsWithNullFlag,
                    true,
                )
                .expect("mirrored u64 encoding"),
                builtin: EncodingRuntimeRegistry::seeded()
                    .expect("seeded encoding runtimes")
                    .resolve(ENCODING_U64_ID)
                    .expect("builtin u64 encoding")
                    .clone(),
            }
        }
    }

    impl EncodingRuntime for MirroredU64EncodingRuntime {
        fn encoding_profile_id(&self) -> EncodingProfileId {
            self.descriptor.encoding_profile_id
        }

        fn descriptor(&self) -> &EncodingProfile {
            &self.descriptor
        }

        fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
            let builtin = TypedValue::new(TYPE_U64_ID, value.payload().to_vec());
            self.builtin.encode_field_elements(&builtin)
        }

        fn decode_field_elements(
            &self,
            field_elements: &[KoalaBear],
        ) -> Result<TypedValue, TabulaError> {
            let builtin = self.builtin.decode_field_elements(field_elements)?;
            Ok(TypedValue::new(
                self.descriptor.type_id,
                builtin.payload().to_vec(),
            ))
        }

        fn encode_transcript_atoms(
            &self,
            value: &TypedValue,
        ) -> Result<Vec<KoalaBear>, TabulaError> {
            let builtin = TypedValue::new(TYPE_U64_ID, value.payload().to_vec());
            self.builtin.encode_transcript_atoms(&builtin)
        }

        fn trace_width(&self) -> usize {
            self.descriptor.width as usize
        }
    }

    fn event() -> PrecompileEvent {
        PrecompileEvent {
            tx_index: 0,
            instruction_index: 0,
            precompile_id: 0x0001,
            inputs: vec![u64_portable(7)],
            outputs: vec![u64_portable(11)],
        }
    }

    fn built_in_u64_signature() -> PrecompileSignature {
        PrecompileSignature::new(
            vec![PrecompileValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: tabula_profile::ENCODING_U64_ID,
            }],
            vec![PrecompileValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: tabula_profile::ENCODING_U64_ID,
            }],
        )
    }

    fn alt_u64_signature() -> PrecompileSignature {
        PrecompileSignature::new(
            vec![PrecompileValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: ALT_U64_ENCODING_ID,
            }],
            vec![PrecompileValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: ALT_U64_ENCODING_ID,
            }],
        )
    }

    fn mirrored_u64_signature(
        type_id: TypeId,
        encoding_profile_id: EncodingProfileId,
    ) -> PrecompileSignature {
        PrecompileSignature::new(
            vec![PrecompileValueProfile {
                type_id,
                encoding_profile_id,
            }],
            vec![PrecompileValueProfile {
                type_id,
                encoding_profile_id,
            }],
        )
    }

    fn mirrored_event(type_id: TypeId) -> PrecompileEvent {
        PrecompileEvent {
            tx_index: 0,
            instruction_index: 0,
            precompile_id: 0x0001,
            inputs: vec![PortableValue::new(
                type_id,
                u64_portable(7).payload().to_vec(),
            )],
            outputs: vec![PortableValue::new(
                type_id,
                u64_portable(11).payload().to_vec(),
            )],
        }
    }

    #[test]
    fn precompile_transcript_round_trips_built_in_signature() {
        let event = event();
        let signature = built_in_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");

        let call = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript call");
        let header = compute_precompile_call_header(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript header");

        assert_eq!(call.header, header);
        assert!(
            !call.payload.is_empty(),
            "typed payload should be populated"
        );
    }

    #[test]
    fn precompile_transcript_digest_changes_when_signature_encoding_changes() {
        let event = event();
        let signature = built_in_u64_signature();
        let alt_signature = alt_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let mut encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        encoding_runtimes
            .register(Arc::new(AltU64EncodingRuntime::new()))
            .expect("register alt encoding runtime");

        let built_in = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("built-in transcript call");
        let alt = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &alt_signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("alt transcript call");

        assert_ne!(built_in.header.event_digest, alt.header.event_digest);
    }

    #[test]
    fn precompile_transcript_rejects_value_type_mismatch() {
        let event = PrecompileEvent {
            tx_index: 0,
            instruction_index: 0,
            precompile_id: 0x0001,
            inputs: vec![u64_portable(7)],
            outputs: vec![bool_portable(true)],
        };
        let signature = built_in_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");

        let err = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect_err("mismatched output type must fail closed");
        assert!(err.to_string().contains("declares type"));
    }

    #[test]
    fn precompile_transcript_payload_changes_when_high_bits_of_type_id_change() {
        let signature_a = mirrored_u64_signature(HIGH_TYPE_ID_A, SHARED_HIGH_TYPE_ENCODING_ID);
        let signature_b = mirrored_u64_signature(HIGH_TYPE_ID_B, SHARED_HIGH_TYPE_ENCODING_ID);
        let event_a = mirrored_event(HIGH_TYPE_ID_A);
        let event_b = mirrored_event(HIGH_TYPE_ID_B);
        let mut type_runtimes_a = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let mut type_runtimes_b = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let mut encoding_runtimes_a =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        let mut encoding_runtimes_b =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        type_runtimes_a
            .register(Arc::new(MirroredU64TypeRuntime::new(
                HIGH_TYPE_ID_A,
                "mirror_u64_a",
            )))
            .expect("register mirrored type runtime a");
        type_runtimes_b
            .register(Arc::new(MirroredU64TypeRuntime::new(
                HIGH_TYPE_ID_B,
                "mirror_u64_b",
            )))
            .expect("register mirrored type runtime b");
        encoding_runtimes_a
            .register(Arc::new(MirroredU64EncodingRuntime::new(
                HIGH_TYPE_ID_A,
                SHARED_HIGH_TYPE_ENCODING_ID,
                "mirror_u64_encoding_shared",
            )))
            .expect("register mirrored encoding runtime a");
        encoding_runtimes_b
            .register(Arc::new(MirroredU64EncodingRuntime::new(
                HIGH_TYPE_ID_B,
                SHARED_HIGH_TYPE_ENCODING_ID,
                "mirror_u64_encoding_shared",
            )))
            .expect("register mirrored encoding runtime b");

        let call_a = PrecompileTranscriptCall::from_event(
            &event_a,
            0x0001,
            &signature_a,
            &type_runtimes_a,
            &encoding_runtimes_a,
        )
        .expect("build transcript call a");
        let call_b = PrecompileTranscriptCall::from_event(
            &event_b,
            0x0001,
            &signature_b,
            &type_runtimes_b,
            &encoding_runtimes_b,
        )
        .expect("build transcript call b");

        assert_ne!(call_a.payload, call_b.payload);
        assert_ne!(call_a.header.event_digest, call_b.header.event_digest);
    }

    #[test]
    fn precompile_transcript_payload_changes_when_high_bits_of_encoding_id_change() {
        let signature_a = mirrored_u64_signature(TYPE_U64_ID, HIGH_ENCODING_ID_A);
        let signature_b = mirrored_u64_signature(TYPE_U64_ID, HIGH_ENCODING_ID_B);
        let event = event();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let mut encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        encoding_runtimes
            .register(Arc::new(MirroredU64EncodingRuntime::new(
                TYPE_U64_ID,
                HIGH_ENCODING_ID_A,
                "mirror_u64_encoding_high_a",
            )))
            .expect("register mirrored encoding runtime a");
        encoding_runtimes
            .register(Arc::new(MirroredU64EncodingRuntime::new(
                TYPE_U64_ID,
                HIGH_ENCODING_ID_B,
                "mirror_u64_encoding_high_b",
            )))
            .expect("register mirrored encoding runtime b");

        let call_a = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &signature_a,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript call a");
        let call_b = PrecompileTranscriptCall::from_event(
            &event,
            0x0001,
            &signature_b,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript call b");

        assert_ne!(call_a.payload, call_b.payload);
        assert_ne!(call_a.header.event_digest, call_b.header.event_digest);
    }
}
