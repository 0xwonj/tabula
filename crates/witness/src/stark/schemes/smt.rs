//! SMT-specific STARK witness assembly from logical per-column inputs.

use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::{
    SMT_STATE_WITNESS_LABEL, SmtStatePathWitness, SmtStateWitness,
};
use tabula_commitment::primitives::{COL_DATA_SMT_DEPTH, DOMAIN_SMT};
use tabula_commitment::schemes::smt::SparseMerkleTree;
use tabula_commitment::{
    ColumnRootBinding, FieldHasher, NativeDigest, NormalizedVerifierDigest, PoseidonHasher,
};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, Digest, RootProfileId, RowKey, TableId};
use tabula_stark::trace::WitnessStore;
use tabula_types::{EncodingRuntime, TypeRuntime, encode_value_with_null_flag};

use super::super::memory::{
    prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,
};
use crate::{AccessEvent, ColumnWrite, CommittedEntry, InitCell};

/// Input bundle for preparing one SMT column proof store.
#[derive(Clone)]
pub struct SmtProofInput<'a> {
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
    /// Root-binding family selected by the sealed profile.
    pub root_binding_family: RootProfileId,
    /// Sealed column profile hash.
    pub column_profile_hash: Digest,
    /// Canonical binding digest for the column.
    pub binding_digest: NativeDigest,
}

type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Prepared native witness artifacts for one SMT-backed column proof.
pub struct PreparedSmtProof {
    /// Canonical root-binding statement for the column.
    pub root_binding: ColumnRootBinding,
    /// Witness-store payload consumed by downstream chips.
    pub store: WitnessStore,
}

impl std::fmt::Debug for PreparedSmtProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSmtProof")
            .field("root_binding", &self.root_binding)
            .finish_non_exhaustive()
    }
}

/// Prepare one SMT proof input bundle into native witness artifacts.
pub fn prepare_smt_proof<const W: usize>(
    input: &SmtProofInput<'_>,
) -> Result<PreparedSmtProof, TabulaError> {
    let hasher = PoseidonHasher::new();
    let old_state = build_smt_state(
        &hasher,
        encode_committed_entries(input.encoding_runtime, input.old_entries)?,
    )?;
    let com_old = old_state.root();
    let is_empty_old = old_state.is_empty();
    let new_state = if input.is_touched {
        apply_smt_writes(
            &hasher,
            &old_state,
            &encode_writes(input.encoding_runtime, input.writes)?,
        )?
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
        new_digest: NormalizedVerifierDigest::new(new_state.root()),
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched: input.is_touched,
    };

    let memory_rows = prepare_memory_shard_rows_from_parts::<W>(
        input.table,
        input.col,
        input.type_runtime,
        input.encoding_runtime,
        input.init_cells,
        input.access_events,
    )?;
    let meta_row = prepare_meta_shard_row_from_parts(&root_binding, input.access_events, true);
    let shared = SharedColumnWitness {
        memory_rows,
        meta_row: (meta_row.is_touched || meta_row.empty_read_count > 0).then_some(meta_row),
    };
    let state_witness = build_smt_state_witness::<W>(&SmtStateWitnessParts {
        column: (input.table, input.col),
        type_runtime: input.type_runtime,
        encoding_runtime: input.encoding_runtime,
        init_cells: input.init_cells,
        writes: input.writes,
        root_binding: &root_binding,
        old_state: &old_state,
        new_state: &new_state,
    })?;

    let mut store = WitnessStore::new();
    store.put(SHARED_COLUMN_WITNESS_LABEL, shared);
    store.put(SMT_STATE_WITNESS_LABEL, state_witness);

    Ok(PreparedSmtProof {
        root_binding,
        store,
    })
}

struct SmtStateWitnessParts<'a> {
    column: (TableId, ColId),
    type_runtime: &'a dyn TypeRuntime,
    encoding_runtime: &'a dyn EncodingRuntime,
    init_cells: &'a [InitCell],
    writes: &'a [ColumnWrite],
    root_binding: &'a ColumnRootBinding,
    old_state: &'a SparseMerkleTree<PoseidonHasher>,
    new_state: &'a SparseMerkleTree<PoseidonHasher>,
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

fn build_smt_state(
    hasher: &PoseidonHasher,
    entries: Vec<(RowKey, Vec<KoalaBear>)>,
) -> Result<SparseMerkleTree<PoseidonHasher>, TabulaError> {
    let mut tree = SparseMerkleTree::new(hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);
    let mut seen = BTreeSet::new();
    for (key, value_fes) in entries {
        if !seen.insert(key) {
            return Err(TabulaError::ConsistencyError(format!(
                "duplicate SMT entry key: {}",
                key.0
            )));
        }
        let leaf = hasher.hash(&value_fes);
        tree.insert(key.0, leaf)?;
    }
    Ok(tree)
}

fn apply_smt_writes(
    hasher: &PoseidonHasher,
    old_tree: &SparseMerkleTree<PoseidonHasher>,
    writes: &[(RowKey, Option<Vec<KoalaBear>>)],
) -> Result<SparseMerkleTree<PoseidonHasher>, TabulaError> {
    let mut tree = old_tree.clone();
    for (key, value) in writes {
        match value {
            Some(fes) => {
                let leaf = hasher.hash(fes);
                tree.insert(key.0, leaf)?;
            }
            None => {
                tree.remove(key.0)?;
            }
        }
    }
    Ok(tree)
}

fn build_smt_state_witness<const W: usize>(
    parts: &SmtStateWitnessParts<'_>,
) -> Result<SmtStateWitness<W>, TabulaError> {
    let (table, col) = parts.column;
    let old_tree = parts.old_state;
    let new_tree = parts.new_state;

    let init_by_key = collect_init_cells::<W>(
        table,
        col,
        parts.type_runtime,
        parts.encoding_runtime,
        parts.init_cells,
    )?;
    let writes_by_key = collect_final_writes::<W>(
        table,
        col,
        parts.type_runtime,
        parts.encoding_runtime,
        parts.writes,
    )?;

    let mut keys: BTreeSet<_> = init_by_key.keys().copied().collect();
    keys.extend(writes_by_key.keys().copied());

    let mut paths = Vec::with_capacity(keys.len());
    let hasher = PoseidonHasher::new();
    let empty_leaf = hasher.hash_domain(DOMAIN_SMT, &[]);
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

        validate_leaf_match::<W>(
            "old",
            key,
            &old_proof.value,
            &old_val,
            old_is_null,
            &hasher,
            empty_leaf,
        )?;
        validate_leaf_match::<W>(
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
            path_bits: path_bits_from_key(key.0).to_vec(),
        });
    }

    Ok(SmtStateWitness {
        table_id: table.0,
        col_id: col.0,
        column_old_root: parts.root_binding.old_digest.digest,
        column_new_root: parts.root_binding.new_digest.digest,
        column_is_empty_old: parts.root_binding.is_empty_old,
        column_is_empty_new: parts.root_binding.is_empty_new,
        column_is_touched: parts.root_binding.is_touched,
        paths,
    })
}

fn collect_init_cells<const W: usize>(
    table: TableId,
    col: ColId,
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    init_cells: &[InitCell],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    init_cells
        .iter()
        .map(|cell| {
            let value =
                encode_array::<W>(type_runtime, encoding_runtime, &cell.value, cell.is_null)
                    .map_err(|_| TabulaError::ProofError {
                        phase: "smt_proof",
                        detail: format!(
                            "init cell width mismatch for table {} col {} key {}",
                            table.0, col.0, cell.key.row.0
                        ),
                    })?;
            Ok((cell.key.row, (value, cell.is_null)))
        })
        .collect()
}

fn collect_final_writes<const W: usize>(
    table: TableId,
    col: ColId,
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    writes: &[ColumnWrite],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    let mut encoded_writes = BTreeMap::new();
    let zero_typed = type_runtime.zero_typed();
    for write in writes {
        let (value, is_null) = match &write.value {
            Some(value) => (value, false),
            None => (&zero_typed, true),
        };
        let encoded =
            encode_array::<W>(type_runtime, encoding_runtime, value, is_null).map_err(|_| {
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
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    value: &tabula_types::TypedValue,
    is_null: bool,
) -> Result<[KoalaBear; W], TabulaError> {
    let (mut encoded, _) =
        encode_value_with_null_flag(type_runtime, encoding_runtime, value, is_null)?;
    if encoded.len() > W {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "value encoded width {} exceeds SMT witness width {}",
                encoded.len(),
                W
            ),
        });
    }
    encoded.resize(W, KoalaBear::ZERO);
    encoded.try_into().map_err(|_| TabulaError::ProofError {
        phase: "smt_proof",
        detail: format!("expected exactly {W} field elements after padding"),
    })
}

fn validate_leaf_match<const W: usize>(
    label: &str,
    key: RowKey,
    proof_value: &Option<NativeDigest>,
    witness_value: &[KoalaBear; W],
    witness_is_null: bool,
    hasher: &PoseidonHasher,
    empty_leaf: NativeDigest,
) -> Result<(), TabulaError> {
    let expected = if witness_is_null {
        empty_leaf
    } else {
        hasher.hash(witness_value)
    };
    let actual = proof_value.as_ref().copied().unwrap_or(empty_leaf);
    if expected != actual {
        return Err(TabulaError::ProofError {
            phase: "smt_proof",
            detail: format!(
                "{label} SMT proof leaf mismatch for row {}: expected {:?}, got {:?}",
                key.0, expected, actual
            ),
        });
    }
    Ok(())
}

fn path_bits_from_key(key: u64) -> [bool; COL_DATA_SMT_DEPTH] {
    core::array::from_fn(|idx| ((key >> idx) & 1) == 1)
}
