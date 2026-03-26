//! Shared typed-tuple transcript encoding for compiler sealing and proof prep.

use borsh::{BorshDeserialize, BorshSerialize};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use serde::{Deserialize, Serialize};

use tabula_core::error::TabulaError;
use tabula_core::{EncodingProfileId, TypeId};

use crate::format::static_tables::compute_block_chain_digest_from_iter;

/// Fixed tuple slot capacity used by canonical tuple transcripts.
pub const TYPED_TUPLE_MAX_SLOTS: usize = 16;
/// Fixed field-element width used for one encoded tuple element.
pub const TYPED_TUPLE_VALUE_WIDTH: usize = 3;
/// Domain tag for the typed tuple field transcript.
pub const TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG: u32 = 0x51;
/// Number of field elements absorbed per transcript block.
pub const TYPED_TUPLE_TRANSCRIPT_RATE: usize = 8;
/// Fixed number of metadata/value fields per tuple slot.
pub const TYPED_TUPLE_SLOT_FIELD_WIDTH: usize = 2 + TYPED_TUPLE_VALUE_WIDTH;
/// Total number of field elements in the canonical tuple schedule.
pub const TYPED_TUPLE_TOTAL_FIELDS: usize =
    3 + TYPED_TUPLE_MAX_SLOTS * TYPED_TUPLE_SLOT_FIELD_WIDTH;
/// Number of transcript blocks used by the fixed tuple schedule.
pub const TYPED_TUPLE_BLOCKS: usize =
    TYPED_TUPLE_TOTAL_FIELDS.div_ceil(TYPED_TUPLE_TRANSCRIPT_RATE);

/// Domain-separated typed tuple role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedTupleRole {
    /// Relation input tuple.
    RelationInput,
    /// Relation output tuple.
    RelationOutput,
}

impl TypedTupleRole {
    /// Small numeric tag used in AIR relays.
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::RelationInput => 1,
            Self::RelationOutput => 2,
        }
    }
}

/// Sealed default tuple-encoding choice for one semantic type.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TupleEncodingSelection {
    /// Semantic type id.
    pub type_id: TypeId,
    /// Encoding profile id sealed for tuple/static-table digests.
    pub encoding_profile_id: EncodingProfileId,
}

/// Deterministic sealed tuple-encoding defaults shared across compiler,
/// runtime, witness, chips, and verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TupleEncodingDefaults {
    /// Canonically ordered type -> encoding selections.
    pub entries: Vec<TupleEncodingSelection>,
}

impl TupleEncodingDefaults {
    /// Build one canonical tuple-encoding defaults map from `(type, encoding)`
    /// pairs.
    pub fn new(entries: Vec<TupleEncodingSelection>) -> Result<Self, TabulaError> {
        let mut entries = entries;
        entries.sort_unstable_by_key(|entry| entry.type_id);
        for window in entries.windows(2) {
            if window[0].type_id == window[1].type_id {
                return Err(TabulaError::Custom(format!(
                    "duplicate tuple encoding default for type {}",
                    window[0].type_id.0
                )));
            }
        }
        Ok(Self { entries })
    }

    /// Resolve the sealed default encoding profile for one semantic type.
    pub fn resolve(&self, type_id: TypeId) -> Result<EncodingProfileId, TabulaError> {
        self.entries
            .binary_search_by_key(&type_id, |entry| entry.type_id)
            .map(|index| self.entries[index].encoding_profile_id)
            .map_err(|_| {
                TabulaError::Custom(format!(
                    "missing tuple encoding default for type {}",
                    type_id.0
                ))
            })
    }

    /// Borrow the canonical ordered entries.
    #[must_use]
    pub fn entries(&self) -> &[TupleEncodingSelection] {
        &self.entries
    }
}

/// One explicitly encoded tuple element consumed by the canonical contract
/// transcript layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTypedTupleElement {
    /// Semantic type id for this tuple position.
    pub type_id: TypeId,
    /// Field-element payload for this position before fixed-width padding.
    pub field_elements: Vec<KoalaBear>,
}

/// Fixed field-oriented witness for one typed tuple transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedTypedTuple {
    /// Tuple role.
    pub role: TypedTupleRole,
    /// Prefix occupancy flags.
    pub used: [bool; TYPED_TUPLE_MAX_SLOTS],
    /// Semantic type ids per tuple position.
    pub type_ids: [u32; TYPED_TUPLE_MAX_SLOTS],
    /// Padded execution-width field encodings per tuple position.
    pub values: [[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
    /// Canonical fixed-size block schedule absorbed by the transcript chip.
    pub blocks: [[KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE]; TYPED_TUPLE_BLOCKS],
    /// Final transcript digest.
    pub digest: [u32; 8],
}

impl MaterializedTypedTuple {
    /// Tuple arity recovered from the prefix occupancy flags.
    #[must_use]
    pub fn arity(&self) -> u32 {
        self.used.iter().filter(|used| **used).count() as u32
    }
}

/// Materialize one typed tuple into its canonical field schedule and digest.
pub fn materialize_typed_tuple(
    role: TypedTupleRole,
    values: &[EncodedTypedTupleElement],
) -> Result<MaterializedTypedTuple, TabulaError> {
    if values.len() > TYPED_TUPLE_MAX_SLOTS {
        return Err(TabulaError::ProofError {
            phase: "typed_tuple_transcript",
            detail: format!(
                "tuple arity {} exceeds TYPED_TUPLE_MAX_SLOTS ({TYPED_TUPLE_MAX_SLOTS})",
                values.len()
            ),
        });
    }

    let mut used = [false; TYPED_TUPLE_MAX_SLOTS];
    let mut type_ids = [0u32; TYPED_TUPLE_MAX_SLOTS];
    let mut encoded_values = [[KoalaBear::ZERO; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS];

    for (index, value) in values.iter().enumerate() {
        let mut encoded = value.field_elements.clone();
        if encoded.len() > TYPED_TUPLE_VALUE_WIDTH {
            return Err(TabulaError::ProofError {
                phase: "typed_tuple_transcript",
                detail: format!(
                    "tuple element type {} encoded width {} exceeds transcript width {}",
                    value.type_id.0,
                    encoded.len(),
                    TYPED_TUPLE_VALUE_WIDTH,
                ),
            });
        }
        encoded.resize(TYPED_TUPLE_VALUE_WIDTH, KoalaBear::ZERO);
        used[index] = true;
        type_ids[index] = value.type_id.0;
        encoded_values[index].copy_from_slice(&encoded[..TYPED_TUPLE_VALUE_WIDTH]);
    }

    let blocks = build_typed_tuple_blocks(role, &used, &type_ids, &encoded_values);
    let digest = compute_typed_tuple_digest_from_blocks(&blocks);

    Ok(MaterializedTypedTuple {
        role,
        used,
        type_ids,
        values: encoded_values,
        blocks,
        digest,
    })
}

/// Canonical digest for one typed tuple.
pub fn compute_typed_tuple_digest(
    role: TypedTupleRole,
    values: &[EncodedTypedTupleElement],
) -> Result<[u32; 8], TabulaError> {
    Ok(materialize_typed_tuple(role, values)?.digest)
}

/// Canonical block schedule for one tuple witness.
pub fn build_typed_tuple_blocks(
    role: TypedTupleRole,
    used: &[bool; TYPED_TUPLE_MAX_SLOTS],
    type_ids: &[u32; TYPED_TUPLE_MAX_SLOTS],
    values: &[[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
) -> [[KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE]; TYPED_TUPLE_BLOCKS] {
    let mut fields = [KoalaBear::ZERO; TYPED_TUPLE_TOTAL_FIELDS];
    fields[0] = KoalaBear::new(TYPED_TUPLE_TRANSCRIPT_DOMAIN_TAG);
    fields[1] = KoalaBear::new(role.as_u32());
    fields[2] = KoalaBear::new(used.iter().filter(|flag| **flag).count() as u32);

    let mut cursor = 3;
    for ((is_used, type_id), value_limbs) in used.iter().zip(type_ids).zip(values) {
        fields[cursor] = if *is_used {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cursor += 1;
        fields[cursor] = KoalaBear::new(*type_id);
        cursor += 1;
        for value in value_limbs {
            fields[cursor] = *value;
            cursor += 1;
        }
    }

    let mut blocks = [[KoalaBear::ZERO; TYPED_TUPLE_TRANSCRIPT_RATE]; TYPED_TUPLE_BLOCKS];
    for (block_index, chunk) in fields.chunks(TYPED_TUPLE_TRANSCRIPT_RATE).enumerate() {
        let chunk_len = chunk.len();
        blocks[block_index][..chunk_len].copy_from_slice(chunk);
    }
    blocks
}

/// Deterministic Poseidon block-chain digest for one fixed block schedule.
#[must_use]
pub fn compute_typed_tuple_digest_from_blocks<const BLOCKS: usize>(
    blocks: &[[KoalaBear; TYPED_TUPLE_TRANSCRIPT_RATE]; BLOCKS],
) -> [u32; 8] {
    compute_block_chain_digest_from_iter(blocks.iter())
}
