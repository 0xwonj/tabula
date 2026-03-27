//! Dedicated transcript lane for relation input/output tuples.
//!
//! This chip proves a fixed field-oriented transcript over typed tuple metadata
//! and padded value limbs. Execution binds the tuple columns; this lane binds
//! the final digest.
#![allow(unused_imports)]

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
