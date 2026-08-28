//! Generic transcript lane for capability calls.
//!
//! This chip proves a canonical, length-delimited transcript digest for each
//! capability call, then relays the call header on the shared CAPABILITY_TRANSCRIPT bus.
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{
    CapabilityCallEvent, CapabilityTranscriptSignature, CapabilityTranscriptValueProfile,
    PortableValue,
};
use tabula_stark::chips::ChipId;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::poseidon::constants::poseidon2_permutation;

/// Witness-store label for transcript calls consumed by `CapabilityTranscriptChip`.
pub const CAPABILITY_TRANSCRIPT_WITNESS_LABEL: &str = "capability_transcript_calls";

/// Fixed chip id for the generic capability transcript lane.
pub const CAPABILITY_TRANSCRIPT_CHIP_ID: ChipId = ChipId(90);

/// Domain tag for the first transcript row of one capability call.
pub const CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG: u32 = 0x31;
/// Domain tag for continuation transcript rows of one capability call.
pub const CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG: u32 = 0x32;

pub(super) const FIRST_ROW_PAYLOAD_CAPACITY: usize = 8;
pub(super) const CONT_ROW_PAYLOAD_CAPACITY: usize = 5;
pub(super) const CAPABILITY_TRANSCRIPT_WIDTH: usize = 43;

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

pub(super) fn build_first_row_perm_input(
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

pub(super) fn build_cont_row_perm_input(
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
