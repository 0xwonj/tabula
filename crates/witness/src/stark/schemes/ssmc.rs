//! SSMC-specific STARK witness assembly from logical per-column inputs.

use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::property::trace::PROPERTY_READ_WITNESS_LABEL;
use tabula_chips::shards::property::trace::PropertyReadRecord;
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_commitment::schemes::tags;
use tabula_commitment::{ColumnMeta, ColumnState, KoalaBearCodec, PoseidonHasher};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{CellKey, ColId, RowKey, TableId, Value, ValueType};
use tabula_stark::trace::WitnessStore;

use super::super::memory::{SsmcColumnWitnessParts, prepare_ssmc_column_witness_from_parts};
use crate::{AccessEvent, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim};

/// Input bundle for preparing one SSMC column proof store.
#[derive(Clone, Copy, Debug)]
pub struct SsmcProofInput<'a> {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Column value type.
    pub value_type: ValueType,
    /// Old committed-state entries for the column.
    pub old_entries: &'a [CommittedEntry],
    /// Base-state init cells grouped for this column.
    pub init_cells: &'a [InitCell],
    /// Execution access events for this column.
    pub access_events: &'a [AccessEvent],
    /// Final coalesced writes for this column.
    pub writes: &'a [ColumnWrite],
    /// Whether the batch contains at least one effective final write.
    pub is_touched: bool,
    /// Property-read claims for this column.
    pub property_reads: &'a [PropertyReadClaim],
}

type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Prepared STARK proof product for one SSMC column.
pub struct PreparedSsmcProof {
    /// Verifier-visible column metadata.
    pub meta: ColumnMeta,
    /// Column-tier witness store for the current backend.
    pub store: WitnessStore,
}

impl std::fmt::Debug for PreparedSsmcProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSsmcProof")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// Build the full SSMC per-column proof product from logical inputs.
pub fn prepare_ssmc_proof<const W: usize>(
    input: SsmcProofInput<'_>,
) -> Result<PreparedSsmcProof, TabulaError> {
    let hasher = PoseidonHasher::new();
    let (old_state, _) = ColumnState::commit(
        &hasher,
        input.table,
        input.col,
        encode_committed_entries(input.old_entries)?,
        tags::SSMC,
    )?;
    let com_old = old_state.proof_commitment(input.table, input.col)?;
    let is_empty_old = old_state.is_empty();
    let (new_state, _runtime_com_new) = if input.is_touched {
        old_state.apply_writes(
            &hasher,
            input.table,
            input.col,
            &encode_writes(input.writes)?,
        )?
    } else {
        (old_state.clone(), com_old)
    };
    let meta = ColumnMeta {
        table: input.table,
        col: input.col,
        tag: tags::SSMC,
        com_old,
        com_new: new_state.proof_commitment(input.table, input.col)?,
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched: input.is_touched,
    };

    let property_reads = input
        .property_reads
        .iter()
        .map(encode_property_record::<W>)
        .collect::<Result<Vec<_>, _>>()?;
    let init_cells = if property_reads.is_empty() {
        input.init_cells.to_vec()
    } else {
        synthesize_old_init_cells(input.table, input.col, &old_state, input.value_type)?
    };

    let old_entries = ssmc_entries(&old_state)?;
    let new_entries = ssmc_entries(&new_state)?;
    let column_witness = prepare_ssmc_column_witness_from_parts::<W>(&SsmcColumnWitnessParts {
        column: (input.table, input.col),
        value_type: input.value_type,
        init_cells: &init_cells,
        access_events: input.access_events,
        old_entries: &old_entries,
        new_entries: &new_entries,
        meta: &meta,
        has_commitment_proof: true,
    })?;

    let mut store = WitnessStore::new();
    store.put(
        SHARED_COLUMN_WITNESS_LABEL,
        SharedColumnWitness {
            memory_rows: column_witness.memory_rows.clone(),
            meta_row: column_witness.meta_row.clone(),
        },
    );
    let mut single_witness = SsmcWitness::default();
    single_witness.insert(input.table, input.col, column_witness);
    store.put(SSMC_WITNESS_LABEL, single_witness);
    if !property_reads.is_empty() {
        store.put(PROPERTY_READ_WITNESS_LABEL, property_reads);
    }

    Ok(PreparedSsmcProof { meta, store })
}

fn encode_committed_entries(
    entries: &[CommittedEntry],
) -> Result<Vec<(RowKey, Vec<KoalaBear>)>, TabulaError> {
    let codec = KoalaBearCodec;
    let mut encoded = Vec::new();
    for entry in entries {
        if entry.is_null {
            continue;
        }
        encoded.push((entry.row, codec.encode(&entry.value)?));
    }
    encoded.sort_by_key(|(row, _)| *row);
    Ok(encoded)
}

fn encode_writes(writes: &[ColumnWrite]) -> Result<EncodedWrites, TabulaError> {
    let codec = KoalaBearCodec;
    writes
        .iter()
        .map(|write| {
            Ok((
                write.row,
                write
                    .value
                    .as_ref()
                    .map(|value| codec.encode(value))
                    .transpose()?,
            ))
        })
        .collect()
}

fn synthesize_old_init_cells(
    table: TableId,
    col: ColId,
    state: &ColumnState<PoseidonHasher>,
    value_type: ValueType,
) -> Result<Vec<InitCell>, TabulaError> {
    let codec = KoalaBearCodec;
    ssmc_entries(state)?
        .into_iter()
        .map(|(row, value_fes)| {
            Ok(InitCell {
                key: CellKey { table, col, row },
                value: codec.decode(&value_fes, value_type)?,
                is_null: false,
            })
        })
        .collect()
}

fn encode_property_record<const W: usize>(
    claim: &PropertyReadClaim,
) -> Result<PropertyReadRecord, TabulaError> {
    let codec = KoalaBearCodec;
    let (arg0, arg1) = claim.query.encoded_args();
    Ok(PropertyReadRecord {
        query_type: claim.query.kind_ordinal(),
        query_arg0: encode_padded::<W>(&codec, &Value::U64(arg0))?,
        query_arg1: encode_padded::<W>(&codec, &Value::U64(arg1))?,
        result_val: encode_padded::<W>(&codec, &claim.result.value)?,
        result_key: encode_padded::<W>(
            &codec,
            &Value::U64(claim.result.key.unwrap_or(RowKey(0)).0),
        )?,
        is_null: claim.result.is_null,
    })
}

fn encode_padded<const W: usize>(
    codec: &KoalaBearCodec,
    value: &Value,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let mut encoded = codec.encode(value)?;
    if encoded.len() > W {
        return Err(TabulaError::ProofError {
            phase: "ssmc_proof",
            detail: format!(
                "value encoded width {} exceeds proof width {}",
                encoded.len(),
                W
            ),
        });
    }
    encoded.resize(W, KoalaBear::ZERO);
    Ok(encoded)
}

fn ssmc_entries(
    state: &ColumnState<PoseidonHasher>,
) -> Result<BTreeMap<RowKey, Vec<KoalaBear>>, TabulaError> {
    match state {
        ColumnState::Ssmc(list) => Ok(list
            .entries()
            .iter()
            .map(|entry| (entry.key, entry.value.clone()))
            .collect()),
        ColumnState::Smt(_) => Err(TabulaError::ProofError {
            phase: "ssmc_proof",
            detail: "only SSMC-backed columns are supported".to_string(),
        }),
    }
}
