//! SSMC-specific STARK witness assembly from logical per-column inputs.

use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::property::trace::PROPERTY_READ_WITNESS_LABEL;
use tabula_chips::shards::property::trace::PropertyReadRecord;
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_commitment::schemes::ssmc::SsmcList;
use tabula_commitment::{ColumnRootBinding, NormalizedVerifierDigest, PoseidonHasher};
use tabula_core::error::TabulaError;
use tabula_core::{CellKey, ColId, Digest, RootProfileId, RowKey, TableId};
use tabula_stark::trace::WitnessStore;
use tabula_types::{EncodingRuntime, TypeRuntime, encode_structural_u64};

use super::super::memory::{SsmcColumnWitnessParts, prepare_ssmc_column_witness_from_parts};
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

type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

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
    let hasher = PoseidonHasher::new();
    let old_state = SsmcList::from_sorted(
        input.table,
        input.col,
        encode_committed_entries(input.encoding_runtime, input.old_entries)?
            .into_iter()
            .map(|(key, value)| tabula_commitment::schemes::ssmc::SsmcEntry { key, value })
            .collect(),
    )?;
    let com_old = old_state.proof_commitment()?;
    let is_empty_old = old_state.is_empty();
    let new_state = if input.is_touched {
        old_state
            .apply_writes(
                &encode_writes(input.encoding_runtime, input.writes)?,
                &hasher,
            )
            .0
    } else {
        old_state.clone()
    };
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
        .map(|claim| encode_property_record::<W>(input.encoding_runtime, claim))
        .collect::<Result<Vec<_>, _>>()?;
    let init_cells = if property_reads.is_empty() {
        input.init_cells.to_vec()
    } else {
        synthesize_old_init_cells(
            input.table,
            input.col,
            &old_state,
            input.type_runtime,
            input.encoding_runtime,
        )?
    };

    let old_entries = ssmc_entries(&old_state)?;
    let new_entries = ssmc_entries(&new_state)?;
    let column_witness = prepare_ssmc_column_witness_from_parts::<W>(&SsmcColumnWitnessParts {
        column: (input.table, input.col),
        type_runtime: input.type_runtime,
        encoding_runtime: input.encoding_runtime,
        init_cells: &init_cells,
        access_events: input.access_events,
        old_entries: &old_entries,
        new_entries: &new_entries,
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
    entries: &[CommittedEntry],
) -> Result<Vec<(RowKey, Vec<KoalaBear>)>, TabulaError> {
    let mut encoded = Vec::new();
    for entry in entries {
        if entry.is_null {
            continue;
        }
        encoded.push((
            entry.row,
            encoding_runtime.encode_field_elements(&entry.value)?,
        ));
    }
    encoded.sort_by_key(|(row, _)| *row);
    Ok(encoded)
}

fn encode_writes(
    encoding_runtime: &dyn EncodingRuntime,
    writes: &[ColumnWrite],
) -> Result<EncodedWrites, TabulaError> {
    writes
        .iter()
        .map(|write| {
            Ok((
                write.row,
                write
                    .value
                    .as_ref()
                    .map(|value| encoding_runtime.encode_field_elements(value))
                    .transpose()?,
            ))
        })
        .collect()
}

fn synthesize_old_init_cells(
    table: TableId,
    col: ColId,
    state: &SsmcList,
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
) -> Result<Vec<InitCell>, TabulaError> {
    ssmc_entries(state)?
        .into_iter()
        .map(|(row, value_fes)| {
            let value = encoding_runtime.decode_field_elements(&value_fes)?;
            type_runtime.validate(&value)?;
            Ok(InitCell {
                key: CellKey { table, col, row },
                value,
                is_null: false,
            })
        })
        .collect()
}

fn encode_property_record<const W: usize>(
    encoding_runtime: &dyn EncodingRuntime,
    claim: &PropertyReadClaim,
) -> Result<PropertyReadRecord, TabulaError> {
    let (arg0, arg1) = claim.query.encoded_args();
    Ok(PropertyReadRecord {
        query_type: claim.query.kind_ordinal(),
        query_arg0: encode_structural_u64::<W>(arg0)?,
        query_arg1: encode_structural_u64::<W>(arg1)?,
        result_val: encode_padded_with_encoding::<W>(encoding_runtime, &claim.result.value)?,
        result_key: encode_structural_u64::<W>(claim.result.key.unwrap_or(RowKey(0)).0)?,
        is_null: claim.result.is_null,
    })
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

fn ssmc_entries(state: &SsmcList) -> Result<BTreeMap<RowKey, Vec<KoalaBear>>, TabulaError> {
    Ok(state
        .entries()
        .iter()
        .map(|entry| (entry.key, entry.value.clone()))
        .collect())
}
