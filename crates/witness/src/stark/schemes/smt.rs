//! SMT-specific STARK witness assembly from logical per-column inputs.

use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::{
    SMT_STATE_WITNESS_LABEL, SmtStatePathWitness, SmtStateWitness,
};
use tabula_commitment::primitives::{COL_DATA_SMT_DEPTH, DOMAIN_SMT};
use tabula_commitment::schemes::tags;
use tabula_commitment::{
    ColumnMeta, ColumnState, FieldHasher, KoalaBearCodec, NativeDigest, PoseidonHasher,
};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{ColId, RowKey, TableId, Value, ValueType};
use tabula_stark::trace::WitnessStore;

use super::super::memory::{
    prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,
};
use crate::{AccessEvent, ColumnWrite, CommittedEntry, InitCell};

/// Input bundle for preparing one SMT column proof store.
#[derive(Clone, Copy, Debug)]
pub struct SmtProofInput<'a> {
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
}

type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Prepared STARK proof product for one SMT column.
pub struct PreparedSmtProof {
    /// Verifier-visible column metadata.
    pub meta: ColumnMeta,
    /// Column-tier witness store for the current backend.
    pub store: WitnessStore,
}

impl std::fmt::Debug for PreparedSmtProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSmtProof")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// Build the full SMT per-column proof product from logical inputs.
pub fn prepare_smt_proof<const W: usize>(
    input: SmtProofInput<'_>,
) -> Result<PreparedSmtProof, TabulaError> {
    let hasher = PoseidonHasher::new();
    let (old_state, _) = ColumnState::commit(
        &hasher,
        input.table,
        input.col,
        encode_committed_entries(input.old_entries)?,
        tags::SMT,
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
        tag: tags::SMT,
        com_old,
        com_new: new_state.proof_commitment(input.table, input.col)?,
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched: input.is_touched,
    };

    let memory_rows = prepare_memory_shard_rows_from_parts::<W>(
        input.table,
        input.col,
        input.value_type,
        input.init_cells,
        input.access_events,
    )?;
    let meta_row = prepare_meta_shard_row_from_parts(&meta, input.access_events, true);
    let shared = SharedColumnWitness {
        memory_rows,
        meta_row: (meta_row.is_touched || meta_row.empty_read_count > 0).then_some(meta_row),
    };
    let state_witness = build_smt_state_witness::<W>(&SmtStateWitnessParts {
        column: (input.table, input.col),
        value_type: input.value_type,
        init_cells: input.init_cells,
        writes: input.writes,
        is_touched: meta.is_touched,
        meta: &meta,
        old_state: &old_state,
        new_state: &new_state,
    })?;

    let mut store = WitnessStore::new();
    store.put(SHARED_COLUMN_WITNESS_LABEL, shared);
    store.put(SMT_STATE_WITNESS_LABEL, state_witness);

    Ok(PreparedSmtProof { meta, store })
}

struct SmtStateWitnessParts<'a> {
    column: (TableId, ColId),
    value_type: ValueType,
    init_cells: &'a [InitCell],
    writes: &'a [ColumnWrite],
    is_touched: bool,
    meta: &'a ColumnMeta,
    old_state: &'a ColumnState<PoseidonHasher>,
    new_state: &'a ColumnState<PoseidonHasher>,
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

fn build_smt_state_witness<const W: usize>(
    parts: &SmtStateWitnessParts<'_>,
) -> Result<SmtStateWitness<W>, TabulaError> {
    let (table, col) = parts.column;
    let ColumnState::Smt(old_tree) = parts.old_state else {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "column ({}, {}) is not SMT-backed in old state",
                table.0, col.0
            ),
        });
    };
    let ColumnState::Smt(new_tree) = parts.new_state else {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "column ({}, {}) is not SMT-backed in new state",
                table.0, col.0
            ),
        });
    };

    let init_by_key = collect_init_cells::<W>(table, col, parts.value_type, parts.init_cells)?;
    let writes_by_key = collect_final_writes::<W>(table, col, parts.value_type, parts.writes)?;

    let mut keys: BTreeSet<_> = init_by_key.keys().copied().collect();
    keys.extend(writes_by_key.keys().copied());

    if parts.is_touched && keys.is_empty() {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "touched SMT column ({}, {}) has no touched keys",
                table.0, col.0,
            ),
        });
    }

    let hasher = PoseidonHasher::new();
    let empty_leaf = hasher.hash_domain(DOMAIN_SMT, &[]);

    let mut paths = Vec::with_capacity(keys.len());
    for key in keys {
        if key.0 >= (1u64 << COL_DATA_SMT_DEPTH) {
            return Err(TabulaError::ProofError {
                phase: "smt_proof",
                detail: format!(
                    "SMT column ({}, {}) key {} exceeds row-level SMT depth {}",
                    table.0, col.0, key.0, COL_DATA_SMT_DEPTH,
                ),
            });
        }

        let (old_val, old_is_null) = init_by_key
            .get(&key)
            .copied()
            .unwrap_or(([KoalaBear::ZERO; W], true));
        let (new_val, new_is_null, write_mult) = writes_by_key
            .get(&key)
            .copied()
            .map_or((old_val, old_is_null, false), |(value, is_null)| {
                (value, is_null, true)
            });

        let old_proof = old_tree.prove(key.0)?;
        let new_proof = new_tree.prove(key.0)?;

        validate_leaf_match(
            "old",
            key,
            &old_proof.value,
            &old_val,
            old_is_null,
            &hasher,
            empty_leaf,
        )?;
        validate_leaf_match(
            "new",
            key,
            &new_proof.value,
            &new_val,
            new_is_null,
            &hasher,
            empty_leaf,
        )?;

        paths.push(SmtStatePathWitness {
            key: key.0,
            old_val,
            new_val,
            old_is_null,
            new_is_null,
            write_mult,
            old_siblings: old_proof.siblings,
            new_siblings: new_proof.siblings,
            path_bits: path_bits_from_key(key.0),
        });
    }

    Ok(SmtStateWitness {
        table_id: table.0,
        col_id: col.0,
        column_old_root: parts.meta.com_old,
        column_new_root: parts.meta.com_new,
        column_is_empty_old: parts.meta.is_empty_old,
        column_is_empty_new: parts.meta.is_empty_new,
        column_is_touched: parts.meta.is_touched,
        paths,
    })
}

fn collect_init_cells<const W: usize>(
    table: TableId,
    col: ColId,
    value_type: ValueType,
    init_cells: &[InitCell],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    let codec = KoalaBearCodec;
    init_cells
        .iter()
        .map(|cell| {
            let value =
                encode_array::<W>(&codec, &cell.value, cell.is_null, value_type).map_err(|_| {
                    TabulaError::ProofError {
                        phase: "smt_proof",
                        detail: format!(
                            "init cell width mismatch for table {} col {} key {}",
                            table.0, col.0, cell.key.row.0
                        ),
                    }
                })?;
            Ok((cell.key.row, (value, cell.is_null)))
        })
        .collect()
}

fn collect_final_writes<const W: usize>(
    table: TableId,
    col: ColId,
    value_type: ValueType,
    writes: &[ColumnWrite],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    let codec = KoalaBearCodec;
    let mut encoded_writes = BTreeMap::new();
    for write in writes {
        let (value, is_null) = match &write.value {
            Some(value) => (value, false),
            None => (&Value::U64(0), true),
        };
        let encoded = encode_array::<W>(&codec, value, is_null, value_type).map_err(|_| {
            TabulaError::ProofError {
                phase: "smt_proof",
                detail: format!(
                    "write event width mismatch for table {} col {} key {}",
                    table.0, col.0, write.row.0
                ),
            }
        })?;
        encoded_writes.insert(write.row, (encoded, is_null));
    }
    Ok(encoded_writes)
}

fn encode_array<const W: usize>(
    codec: &KoalaBearCodec,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<[KoalaBear; W], TabulaError> {
    let mut encoded = if is_null {
        vec![KoalaBear::ZERO; codec.field_elements_per(value_type)]
    } else {
        codec.encode(value)?
    };
    if encoded.len() > W {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "encoded value width {} exceeds proof width {}",
                encoded.len(),
                W
            ),
        });
    }
    encoded.resize(W, KoalaBear::ZERO);
    encoded.try_into().map_err(|_| TabulaError::ProofError {
        phase: "smt_proof",
        detail: format!("failed to convert encoded value to width {W}"),
    })
}

fn validate_leaf_match<const W: usize>(
    phase: &str,
    key: RowKey,
    proof_value: &Option<NativeDigest>,
    value: &[KoalaBear; W],
    is_null: bool,
    hasher: &PoseidonHasher,
    empty_leaf: NativeDigest,
) -> Result<(), TabulaError> {
    let expected = if is_null {
        empty_leaf
    } else {
        hasher.hash(value)
    };
    match proof_value {
        Some(digest) if !is_null && *digest == expected => Ok(()),
        None if is_null => Ok(()),
        other => Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "{phase} SMT proof/value mismatch for key {}: expected_null={} got={other:?}",
                key.0, is_null,
            ),
        }),
    }
}

fn path_bits_from_key(key: u64) -> Vec<bool> {
    (0..COL_DATA_SMT_DEPTH)
        .map(|i| ((key >> i) & 1) == 1)
        .collect()
}
