//! Generic transcript lane for capability calls.
//!
//! This chip proves a canonical, length-delimited transcript digest for each
//! capability call, then relays the call header on the shared CAPABILITY_TRANSCRIPT bus.

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_core::error::TabulaError;
use tabula_core::{
    CapabilityCallEvent, CapabilityTranscriptSignature, CapabilityTranscriptValueProfile,
    PortableValue,
};
use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut};
use tabula_stark::air::interaction::{AirInteraction, core_buses};
use tabula_stark::chips::ChipId;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::poseidon::constants::poseidon2_permutation;

/// Witness-store label for transcript calls consumed by [`CapabilityTranscriptChip`].
pub const CAPABILITY_TRANSCRIPT_WITNESS_LABEL: &str = "capability_transcript_calls";

/// Fixed chip id for the generic capability transcript lane.
pub const CAPABILITY_TRANSCRIPT_CHIP_ID: ChipId = ChipId(90);

/// Domain tag for the first transcript row of one capability call.
pub const CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG: u32 = 0x31;
/// Domain tag for continuation transcript rows of one capability call.
pub const CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG: u32 = 0x32;

const FIRST_ROW_PAYLOAD_CAPACITY: usize = 8;
const CONT_ROW_PAYLOAD_CAPACITY: usize = 5;
const CAPABILITY_TRANSCRIPT_WIDTH: usize = 43;

/// Bus-visible header for one capability call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCallHeader {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Zero-based instruction index in the tx body.
    pub instruction_index: u32,
    /// Capability transcript identifier.
    pub capability_transcript_id: u16,
    /// Number of input values.
    pub input_count: u32,
    /// Number of output values.
    pub output_count: u32,
    /// Canonical transcript digest, encoded as the first 8 Poseidon outputs.
    pub event_digest: [u32; 8],
}

/// Canonical transcript witness for one capability call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityTranscriptCall {
    /// Original execution event.
    pub event: CapabilityCallEvent,
    /// Bus-visible header.
    pub header: CapabilityCallHeader,
    /// Canonically encoded payload (typed inputs, then typed outputs).
    ///
    /// Each value is prefixed by:
    /// `type_id_le32 || encoding_profile_id_le32 || atom_count_le32`,
    /// with every byte represented as one KoalaBear field element.
    pub payload: Vec<KoalaBear>,
}

impl CapabilityTranscriptCall {
    /// Build one canonical transcript witness from a structured execution event.
    pub fn from_event(
        event: &CapabilityCallEvent,
        expected_capability_transcript_id: u16,
        signature: &CapabilityTranscriptSignature,
        type_runtimes: &TypeRuntimeRegistry,
        encoding_runtimes: &EncodingRuntimeRegistry,
    ) -> Result<Self, TabulaError> {
        let (header, payload) = materialize_capability_call_parts(
            event,
            expected_capability_transcript_id,
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
pub fn compute_capability_call_header(
    event: &CapabilityCallEvent,
    expected_capability_transcript_id: u16,
    signature: &CapabilityTranscriptSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<CapabilityCallHeader, TabulaError> {
    materialize_capability_call_parts(
        event,
        expected_capability_transcript_id,
        signature,
        type_runtimes,
        encoding_runtimes,
    )
    .map(|(header, _)| header)
}

fn materialize_capability_call_parts(
    event: &CapabilityCallEvent,
    expected_capability_transcript_id: u16,
    signature: &CapabilityTranscriptSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(CapabilityCallHeader, Vec<KoalaBear>), TabulaError> {
    let payload = encode_capability_call_event_payload(
        event,
        expected_capability_transcript_id,
        signature,
        type_runtimes,
        encoding_runtimes,
    )?;
    let header = build_capability_call_header(
        event,
        expected_capability_transcript_id,
        signature,
        &payload,
    )?;
    Ok((header, payload))
}

fn build_capability_call_header(
    event: &CapabilityCallEvent,
    expected_capability_transcript_id: u16,
    signature: &CapabilityTranscriptSignature,
    payload: &[KoalaBear],
) -> Result<CapabilityCallHeader, TabulaError> {
    validate_event_shape(event, expected_capability_transcript_id, signature)?;
    let tx_index = u32::try_from(event.tx_index).map_err(|_| TabulaError::ProofError {
        phase: "capability_transcript",
        detail: format!("tx_index {} exceeds u32 range", event.tx_index),
    })?;
    let instruction_index =
        u32::try_from(event.instruction_index).map_err(|_| TabulaError::ProofError {
            phase: "capability_transcript",
            detail: format!(
                "instruction_index {} exceeds u32 range",
                event.instruction_index
            ),
        })?;

    let digest = compute_event_digest(
        tx_index,
        instruction_index,
        event.capability_transcript_id,
        event,
        payload,
    );
    Ok(CapabilityCallHeader {
        tx_index,
        instruction_index,
        capability_transcript_id: event.capability_transcript_id,
        input_count: event.inputs.len() as u32,
        output_count: event.outputs.len() as u32,
        event_digest: digest,
    })
}

/// Encode one capability call event into a canonical payload over KoalaBear field elements.
///
/// Each typed value contributes:
/// `type_id_le32 || encoding_profile_id_le32 || atom_count_le32 || transcript_atoms`.
pub fn encode_capability_call_event_payload(
    event: &CapabilityCallEvent,
    expected_capability_transcript_id: u16,
    signature: &CapabilityTranscriptSignature,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<Vec<KoalaBear>, TabulaError> {
    validate_event_shape(event, expected_capability_transcript_id, signature)?;
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
    profiles: &[CapabilityTranscriptValueProfile],
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(), TabulaError> {
    for (idx, (value, profile)) in values.iter().zip(profiles).enumerate() {
        if value.type_id() != profile.type_id {
            return Err(TabulaError::ProofError {
                phase: "capability_transcript",
                detail: format!(
                    "capability value {} declares type {} but event carries type {}",
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
                phase: "capability_transcript",
                detail: format!(
                    "encoding profile {} is incompatible with type {} for capability value {}",
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
                phase: "capability_transcript",
                detail: format!(
                    "capability value {} transcript atom length {} exceeds u32 range",
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
    event: &CapabilityCallEvent,
    expected_capability_transcript_id: u16,
    signature: &CapabilityTranscriptSignature,
) -> Result<(), TabulaError> {
    if event.capability_transcript_id != expected_capability_transcript_id {
        return Err(TabulaError::ProofError {
            phase: "capability_transcript",
            detail: format!(
                "capability call event id 0x{:04x} does not match expected id 0x{:04x}",
                event.capability_transcript_id, expected_capability_transcript_id,
            ),
        });
    }
    if event.inputs.len() != signature.inputs.len() {
        return Err(TabulaError::ProofError {
            phase: "capability_transcript",
            detail: format!(
                "capability 0x{:04x} expects {} inputs but event stores {}",
                event.capability_transcript_id,
                signature.inputs.len(),
                event.inputs.len(),
            ),
        });
    }
    if event.outputs.len() != signature.outputs.len() {
        return Err(TabulaError::ProofError {
            phase: "capability_transcript",
            detail: format!(
                "capability 0x{:04x} expects {} outputs but event stores {}",
                event.capability_transcript_id,
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
    capability_transcript_id: u16,
    event: &CapabilityCallEvent,
    payload: &[KoalaBear],
) -> [u32; 8] {
    let total_payload_len = payload.len();
    let mut chunk_index = 0u32;
    let mut offset = 0usize;
    let first_chunk_len = total_payload_len.min(FIRST_ROW_PAYLOAD_CAPACITY);
    let first_input = build_first_row_perm_input(
        tx_index,
        instruction_index,
        capability_transcript_id,
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
    capability_transcript_id: u16,
    input_count: u32,
    output_count: u32,
    total_payload_len: u32,
    payload_chunk: &[KoalaBear],
) -> [KoalaBear; 16] {
    let mut input = [KoalaBear::ZERO; 16];
    input[0] = KoalaBear::new(CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG);
    input[1] = KoalaBear::new(tx_index);
    input[2] = KoalaBear::new(instruction_index);
    input[3] = KoalaBear::new(capability_transcript_id as u32);
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
    input[0] = KoalaBear::new(CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG);
    input[1] = KoalaBear::new(chunk_index);
    input[2] = KoalaBear::new(payload_chunk.len() as u32);
    input[3..11].copy_from_slice(&prev_digest);
    for (idx, value) in payload_chunk.iter().enumerate() {
        input[11 + idx] = *value;
    }
    input
}

#[repr(C)]
struct CapabilityTranscriptCols<T> {
    is_real: T,
    is_first: T,
    is_last: T,
    tx_index: T,
    instruction_index: T,
    capability_transcript_id: T,
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
struct CapabilityTranscriptRow {
    is_first: bool,
    is_last: bool,
    header: CapabilityCallHeader,
    total_payload_len: u32,
    chunk_index: u32,
    chunk_len: u32,
    prev_digest: [u32; 8],
    perm_input: [KoalaBear; 16],
    perm_output: [u32; 8],
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

impl TraceContributor for CapabilityTranscriptChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let calls =
            store.get::<Vec<CapabilityTranscriptCall>>(CAPABILITY_TRANSCRIPT_WITNESS_LABEL)?;
        let rows = build_transcript_rows(calls);
        let num_real = rows.len();
        let num_rows = (num_real + 1).next_power_of_two().max(2);
        let mut values = vec![KoalaBear::ZERO; num_rows * CAPABILITY_TRANSCRIPT_WIDTH];

        for (row_idx, row) in rows.iter().enumerate() {
            let offset = row_idx * CAPABILITY_TRANSCRIPT_WIDTH;
            let cols: &mut CapabilityTranscriptCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + CAPABILITY_TRANSCRIPT_WIDTH]);
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
            cols.capability_transcript_id =
                KoalaBear::new(row.header.capability_transcript_id as u32);
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
            CAPABILITY_TRANSCRIPT_CHIP_ID,
            RowMajorMatrix::new(values, CAPABILITY_TRANSCRIPT_WIDTH),
        );
        Ok(())
    }
}

fn build_transcript_rows(calls: &[CapabilityTranscriptCall]) -> Vec<CapabilityTranscriptRow> {
    let mut rows = Vec::new();
    for call in calls {
        let total_payload_len = call.payload.len() as u32;
        let first_chunk_len = call.payload.len().min(FIRST_ROW_PAYLOAD_CAPACITY);
        let mut offset = 0usize;
        let mut chunk_index = 0u32;
        let first_input = build_first_row_perm_input(
            call.header.tx_index,
            call.header.instruction_index,
            call.header.capability_transcript_id,
            call.header.input_count,
            call.header.output_count,
            total_payload_len,
            &call.payload[..first_chunk_len],
        );
        let (_, first_output) = poseidon2_permutation(first_input);
        let mut prev_digest = [0u32; 8];
        let mut current_digest: [u32; 8] =
            core::array::from_fn(|idx| first_output[idx].as_canonical_u32());
        rows.push(CapabilityTranscriptRow {
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
            rows.push(CapabilityTranscriptRow {
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
    use tabula_core::CapabilityCallEvent;
    use tabula_core::error::TabulaError;
    use tabula_core::{CapabilityTranscriptSignature, CapabilityTranscriptValueProfile};
    use tabula_core::{EncodingProfileId, PortableValue, TypeId};
    use tabula_profile::{
        CanonicalNullEncoding, ENCODING_U64_ID, EncodingClass, EncodingProfile, FieldFamily,
        GenericIrFamily, HostValueFamily, NullSemantics, TYPE_U64_ID, TranscriptSerialization,
        TypeCapabilities, TypeDescriptor, ZeroValueSpec, builtin_catalog,
    };
    use tabula_types::{
        ArithmeticOp, EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry,
        TypedValue, bool_portable, u64_portable,
    };

    use super::{CapabilityTranscriptCall, compute_capability_call_header};

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

    fn event() -> CapabilityCallEvent {
        CapabilityCallEvent {
            tx_index: 0,
            instruction_index: 0,
            capability_transcript_id: 0x0001,
            inputs: vec![u64_portable(7)],
            outputs: vec![u64_portable(11)],
        }
    }

    fn built_in_u64_signature() -> CapabilityTranscriptSignature {
        CapabilityTranscriptSignature::new(
            vec![CapabilityTranscriptValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: tabula_profile::ENCODING_U64_ID,
            }],
            vec![CapabilityTranscriptValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: tabula_profile::ENCODING_U64_ID,
            }],
        )
    }

    fn alt_u64_signature() -> CapabilityTranscriptSignature {
        CapabilityTranscriptSignature::new(
            vec![CapabilityTranscriptValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: ALT_U64_ENCODING_ID,
            }],
            vec![CapabilityTranscriptValueProfile {
                type_id: TYPE_U64_ID,
                encoding_profile_id: ALT_U64_ENCODING_ID,
            }],
        )
    }

    fn mirrored_u64_signature(
        type_id: TypeId,
        encoding_profile_id: EncodingProfileId,
    ) -> CapabilityTranscriptSignature {
        CapabilityTranscriptSignature::new(
            vec![CapabilityTranscriptValueProfile {
                type_id,
                encoding_profile_id,
            }],
            vec![CapabilityTranscriptValueProfile {
                type_id,
                encoding_profile_id,
            }],
        )
    }

    fn mirrored_event(type_id: TypeId) -> CapabilityCallEvent {
        CapabilityCallEvent {
            tx_index: 0,
            instruction_index: 0,
            capability_transcript_id: 0x0001,
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
    fn capability_transcript_round_trips_built_in_signature() {
        let event = event();
        let signature = built_in_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");

        let call = CapabilityTranscriptCall::from_event(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript call");
        let header = compute_capability_call_header(
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
    fn capability_transcript_digest_changes_when_signature_encoding_changes() {
        let event = event();
        let signature = built_in_u64_signature();
        let alt_signature = alt_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let mut encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
        encoding_runtimes
            .register(Arc::new(AltU64EncodingRuntime::new()))
            .expect("register alt encoding runtime");

        let built_in = CapabilityTranscriptCall::from_event(
            &event,
            0x0001,
            &signature,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("built-in transcript call");
        let alt = CapabilityTranscriptCall::from_event(
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
    fn capability_transcript_rejects_value_type_mismatch() {
        let event = CapabilityCallEvent {
            tx_index: 0,
            instruction_index: 0,
            capability_transcript_id: 0x0001,
            inputs: vec![u64_portable(7)],
            outputs: vec![bool_portable(true)],
        };
        let signature = built_in_u64_signature();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let encoding_runtimes =
            EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");

        let err = CapabilityTranscriptCall::from_event(
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
    fn capability_transcript_payload_changes_when_high_bits_of_type_id_change() {
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

        let call_a = CapabilityTranscriptCall::from_event(
            &event_a,
            0x0001,
            &signature_a,
            &type_runtimes_a,
            &encoding_runtimes_a,
        )
        .expect("build transcript call a");
        let call_b = CapabilityTranscriptCall::from_event(
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
    fn capability_transcript_payload_changes_when_high_bits_of_encoding_id_change() {
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

        let call_a = CapabilityTranscriptCall::from_event(
            &event,
            0x0001,
            &signature_a,
            &type_runtimes,
            &encoding_runtimes,
        )
        .expect("build transcript call a");
        let call_b = CapabilityTranscriptCall::from_event(
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
