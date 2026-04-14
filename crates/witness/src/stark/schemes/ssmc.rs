//! SSMC-specific STARK witness assembly from logical per-column inputs.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::property::trace::PROPERTY_READ_WITNESS_LABEL;
use tabula_chips::shards::property::trace::PropertyReadRecord;
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_commitment::schemes::ssmc::{SsmcEntry, SsmcList};
use tabula_commitment::{ColumnRootBinding, NormalizedVerifierDigest};
use tabula_core::error::TabulaError;
use tabula_core::{
    ColId, CommittedKey, CommittedPropertyQuery, Digest, PropertyAggregateKind, PropertyQueryKind,
    RootProfileId, TableId,
};
use tabula_stark::trace::WitnessStore;
use tabula_types::{
    EncodingRuntime, NativeKeyPayload, TableKeyCodec, TypeRuntime, zero_key_payload,
};

use super::super::memory::{
    OrderedStateEntry, SsmcColumnWitnessParts, prepare_ssmc_column_witness_from_parts,
};
use crate::{AccessEvent, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim};

/// Input bundle for preparing one SSMC column proof store.
#[derive(Clone)]
pub struct SsmcProofInput<'a> {
    /// Column table identifier.
    pub table: TableId,
    /// Column identifier within the table.
    pub col: ColId,
    /// Installed runtime behavior for the column type.
    pub type_runtime: &'a dyn TypeRuntime,
    /// Installed runtime encoding behavior for the column encoding.
    pub encoding_runtime: &'a dyn EncodingRuntime,
    /// Installed runtime key behavior for the table.
    pub key_codec: &'a TableKeyCodec,
    /// Previously committed non-null entries.
    pub old_entries: &'a [CommittedEntry],
    /// Initial cell values materialized for execution.
    pub init_cells: &'a [InitCell],
    /// Logical access events observed during execution.
    pub access_events: &'a [AccessEvent],
    /// Final writes produced by execution.
    pub writes: &'a [ColumnWrite],
    /// Whether the column was touched in this batch.
    pub is_touched: bool,
    /// Prepared property reads for the column.
    pub property_reads: &'a [PropertyReadClaim],
    /// Root-binding family selected by the sealed profile.
    pub root_binding_family: RootProfileId,
    /// Sealed column profile hash.
    pub column_profile_hash: Digest,
    /// Canonical binding digest for the column.
    pub binding_digest: tabula_commitment::NativeDigest,
}

#[derive(Clone)]
struct PreparedSsmcStateEntry {
    key: CommittedKey,
    payload: NativeKeyPayload,
    value: Vec<KoalaBear>,
}

#[derive(Clone)]
struct PreparedSsmcWrite {
    key: CommittedKey,
    payload: NativeKeyPayload,
    value: Option<Vec<KoalaBear>>,
}

/// Prepared native witness artifacts for one SSMC-backed column proof.
pub struct PreparedSsmcProof {
    /// Canonical root-binding statement for the column.
    pub root_binding: ColumnRootBinding,
    /// Witness-store payload consumed by downstream chips.
    pub store: WitnessStore,
}

impl std::fmt::Debug for PreparedSsmcProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSsmcProof")
            .field("root_binding", &self.root_binding)
            .finish_non_exhaustive()
    }
}

/// Prepare one SSMC proof input bundle into native witness artifacts.
pub fn prepare_ssmc_proof<const W: usize>(
    input: &SsmcProofInput<'_>,
) -> Result<PreparedSsmcProof, TabulaError> {
    let old_state_entries =
        encode_committed_entries(input.encoding_runtime, input.key_codec, input.old_entries)?;
    let old_state = SsmcList::from_entries(
        input.table,
        input.col,
        old_state_entries
            .iter()
            .map(|entry| SsmcEntry {
                key: entry.payload,
                value: entry.value.clone(),
            })
            .collect(),
    );
    let com_old = old_state.proof_commitment()?;
    let is_empty_old = old_state.is_empty();
    let new_state_entries = if input.is_touched {
        merge_prepared_entries(
            input.key_codec,
            &old_state_entries,
            &encode_writes(input.encoding_runtime, input.key_codec, input.writes)?,
        )?
    } else {
        old_state_entries.clone()
    };
    let new_state = SsmcList::from_entries(
        input.table,
        input.col,
        new_state_entries
            .iter()
            .map(|entry| SsmcEntry {
                key: entry.payload,
                value: entry.value.clone(),
            })
            .collect(),
    );
    let root_binding = ColumnRootBinding {
        table: input.table,
        col: input.col,
        root_binding_family: input.root_binding_family,
        column_profile_hash: input.column_profile_hash,
        binding_digest: input.binding_digest,
        old_digest: NormalizedVerifierDigest::new(com_old),
        new_digest: NormalizedVerifierDigest::new(new_state.proof_commitment()?),
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched: input.is_touched,
    };

    let property_reads = input
        .property_reads
        .iter()
        .map(|claim| encode_property_record::<W>(input.encoding_runtime, input.key_codec, claim))
        .collect::<Result<Vec<_>, _>>()?;
    let init_cells = if property_reads.is_empty() {
        input.init_cells.to_vec()
    } else {
        synthesize_old_init_cells(input.table, input.col, input.old_entries)?
    };

    let column_witness = prepare_ssmc_column_witness_from_parts::<W>(&SsmcColumnWitnessParts {
        column: (input.table, input.col),
        key_codec: input.key_codec,
        type_runtime: input.type_runtime,
        encoding_runtime: input.encoding_runtime,
        init_cells: &init_cells,
        access_events: input.access_events,
        old_entries: &ordered_state_entries(&old_state_entries),
        new_entries: &ordered_state_entries(&new_state_entries),
        root_binding: &root_binding,
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

    Ok(PreparedSsmcProof {
        root_binding,
        store,
    })
}

fn encode_committed_entries(
    encoding_runtime: &dyn EncodingRuntime,
    key_codec: &TableKeyCodec,
    entries: &[CommittedEntry],
) -> Result<Vec<PreparedSsmcStateEntry>, TabulaError> {
    let mut encoded = Vec::new();
    for entry in entries {
        if entry.is_null {
            continue;
        }
        encoded.push(PreparedSsmcStateEntry {
            key: entry.key.clone(),
            payload: key_codec.encode_padded_proof_payload(&entry.key)?,
            value: encoding_runtime.encode_field_elements(&entry.value)?,
        });
    }
    sort_prepared_entries(key_codec, &mut encoded)?;
    Ok(encoded)
}

fn encode_writes(
    encoding_runtime: &dyn EncodingRuntime,
    key_codec: &TableKeyCodec,
    writes: &[ColumnWrite],
) -> Result<Vec<PreparedSsmcWrite>, TabulaError> {
    let mut encoded: Vec<_> = writes
        .iter()
        .map(|write| {
            Ok(PreparedSsmcWrite {
                key: write.key.clone(),
                payload: key_codec.encode_padded_proof_payload(&write.key)?,
                value: write
                    .value
                    .as_ref()
                    .map(|value| encoding_runtime.encode_field_elements(value))
                    .transpose()?,
            })
        })
        .collect::<Result<_, TabulaError>>()?;
    sort_prepared_writes(key_codec, &mut encoded)?;
    Ok(encoded)
}

fn synthesize_old_init_cells(
    table: TableId,
    col: ColId,
    old_entries: &[CommittedEntry],
) -> Result<Vec<InitCell>, TabulaError> {
    Ok(old_entries
        .iter()
        .filter(|entry| !entry.is_null)
        .map(|entry| InitCell {
            key: tabula_core::CommittedCellKey {
                table,
                col,
                key: entry.key.clone(),
            },
            value: entry.value.clone(),
            is_null: false,
        })
        .collect())
}

fn encode_property_record<const W: usize>(
    encoding_runtime: &dyn EncodingRuntime,
    key_codec: &TableKeyCodec,
    claim: &PropertyReadClaim,
) -> Result<PropertyReadRecord, TabulaError> {
    let (arg0, arg1) = match &claim.query {
        CommittedPropertyQuery::Minimum | CommittedPropertyQuery::Maximum => {
            (zero_key_payload(), zero_key_payload())
        }
        CommittedPropertyQuery::Successor { key } | CommittedPropertyQuery::Predecessor { key } => {
            (
                key_codec.encode_padded_proof_payload(key)?,
                zero_key_payload(),
            )
        }
        CommittedPropertyQuery::NonExistenceRange { lower, upper } => (
            key_codec.encode_padded_proof_payload(lower)?,
            key_codec.encode_padded_proof_payload(upper)?,
        ),
        CommittedPropertyQuery::Aggregate { kind } => {
            let mut payload = zero_key_payload();
            payload[0] = KoalaBear::new(aggregate_kind_ordinal(*kind) as u32);
            (payload, zero_key_payload())
        }
    };
    let result_key = claim
        .result
        .key
        .as_ref()
        .map(|key| key_codec.encode_padded_proof_payload(key))
        .transpose()?
        .unwrap_or_else(zero_key_payload);
    Ok(PropertyReadRecord {
        query_type: property_query_kind(&claim.query).ordinal(),
        query_arg0: arg0.to_vec(),
        query_arg1: arg1.to_vec(),
        result_val: encode_padded_with_encoding::<W>(encoding_runtime, &claim.result.value)?,
        result_key: result_key.to_vec(),
        is_null: claim.result.is_null,
    })
}

fn property_query_kind(query: &CommittedPropertyQuery) -> PropertyQueryKind {
    match query {
        CommittedPropertyQuery::Minimum => PropertyQueryKind::Minimum,
        CommittedPropertyQuery::Maximum => PropertyQueryKind::Maximum,
        CommittedPropertyQuery::Successor { .. } => PropertyQueryKind::Successor,
        CommittedPropertyQuery::Predecessor { .. } => PropertyQueryKind::Predecessor,
        CommittedPropertyQuery::NonExistenceRange { .. } => PropertyQueryKind::NonExistenceRange,
        CommittedPropertyQuery::Aggregate { .. } => PropertyQueryKind::Aggregate,
    }
}

fn aggregate_kind_ordinal(kind: PropertyAggregateKind) -> u64 {
    match kind {
        PropertyAggregateKind::Sum => 0,
        PropertyAggregateKind::Count => 1,
    }
}

fn encode_padded_with_encoding<const W: usize>(
    encoding_runtime: &dyn EncodingRuntime,
    value: &tabula_types::TypedValue,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let mut encoded = encoding_runtime.encode_field_elements(value)?;
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

fn merge_prepared_entries(
    key_codec: &TableKeyCodec,
    old_entries: &[PreparedSsmcStateEntry],
    writes: &[PreparedSsmcWrite],
) -> Result<Vec<PreparedSsmcStateEntry>, TabulaError> {
    let mut merged = Vec::with_capacity(old_entries.len() + writes.len());
    let mut oi = 0usize;
    let mut wi = 0usize;

    while oi < old_entries.len() || wi < writes.len() {
        match (old_entries.get(oi), writes.get(wi)) {
            (Some(old), Some(write)) => match key_codec.compare(&old.key, &write.key)? {
                std::cmp::Ordering::Less => {
                    merged.push(old.clone());
                    oi += 1;
                }
                std::cmp::Ordering::Greater => {
                    if let Some(value) = &write.value {
                        merged.push(PreparedSsmcStateEntry {
                            key: write.key.clone(),
                            payload: write.payload,
                            value: value.clone(),
                        });
                    }
                    wi += 1;
                }
                std::cmp::Ordering::Equal => {
                    if let Some(value) = &write.value {
                        merged.push(PreparedSsmcStateEntry {
                            key: old.key.clone(),
                            payload: old.payload,
                            value: value.clone(),
                        });
                    }
                    oi += 1;
                    wi += 1;
                }
            },
            (Some(old), None) => {
                merged.push(old.clone());
                oi += 1;
            }
            (None, Some(write)) => {
                if let Some(value) = &write.value {
                    merged.push(PreparedSsmcStateEntry {
                        key: write.key.clone(),
                        payload: write.payload,
                        value: value.clone(),
                    });
                }
                wi += 1;
            }
            (None, None) => break,
        }
    }

    Ok(merged)
}

fn ordered_state_entries(entries: &[PreparedSsmcStateEntry]) -> Vec<OrderedStateEntry> {
    entries
        .iter()
        .map(|entry| OrderedStateEntry {
            key: entry.key.clone(),
            payload: entry.payload,
            value: entry.value.clone(),
        })
        .collect()
}

fn sort_prepared_entries(
    key_codec: &TableKeyCodec,
    entries: &mut [PreparedSsmcStateEntry],
) -> Result<(), TabulaError> {
    entries.sort_by(|lhs, rhs| {
        key_codec
            .compare(&lhs.key, &rhs.key)
            .expect("validated state-key ordering must remain available")
    });
    reject_duplicate_ordered_keys(
        key_codec,
        entries.iter().map(|entry| &entry.key),
        "ssmc_proof",
    )
}

fn sort_prepared_writes(
    key_codec: &TableKeyCodec,
    writes: &mut [PreparedSsmcWrite],
) -> Result<(), TabulaError> {
    writes.sort_by(|lhs, rhs| {
        key_codec
            .compare(&lhs.key, &rhs.key)
            .expect("validated state-key ordering must remain available")
    });
    reject_duplicate_ordered_keys(
        key_codec,
        writes.iter().map(|write| &write.key),
        "ssmc_proof",
    )
}

fn reject_duplicate_ordered_keys<'a>(
    key_codec: &TableKeyCodec,
    keys: impl Iterator<Item = &'a CommittedKey>,
    phase: &'static str,
) -> Result<(), TabulaError> {
    let mut prev: Option<&CommittedKey> = None;
    for key in keys {
        if let Some(previous) = prev
            && key_codec.compare(previous, key)? == std::cmp::Ordering::Equal
        {
            return Err(TabulaError::ProofError {
                phase,
                detail: "duplicate committed keys in ordered SSMC proof preparation".into(),
            });
        }
        prev = Some(key);
    }
    Ok(())
}
