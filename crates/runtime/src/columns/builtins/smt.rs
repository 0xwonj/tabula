use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_artifact::SchemeDescriptor;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
#[cfg(feature = "prove")]
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::{
    SMT_STATE_WITNESS_LABEL, SmtStatePathWitness, SmtStateShardChip, SmtStateWitness,
};
#[cfg(feature = "prove")]
use tabula_commitment::{
    COL_DATA_SMT_DEPTH, ColumnMeta, ColumnState, FieldHasher, PoseidonHasher,
    proof_column_commitment, scheme_tags,
};
#[cfg(feature = "prove")]
use tabula_core::error::TabulaError;
use tabula_core::{ColumnLayoutKind, PropertyQueryResult, RowKey, SchemeId, Value};
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_machine::AnyRap;
use tabula_machine::{ColumnChipSet, ProofColumn, SetupError};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::trace::builtin::memory::{
    prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,
};

use crate::columns::{ColumnPlan, ColumnSchemeFactory, ColumnViews, RuntimeColumn};
#[cfg(feature = "prove")]
use crate::columns::{ColumnProofInput, ColumnTransitionBackend, ColumnTransitionInput};

/// SMT commitment scheme factory.
pub struct SmtScheme<const W: usize>;

impl<const W: usize> ColumnSchemeFactory for SmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor::builtin_smt()
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SMT_V1 {
            return Err(SetupError::SetupFailed(format!(
                "scheme factory '{}' cannot prepare column layout {}",
                self.name(),
                plan.scheme_descriptor.layout_kind.0,
            )));
        }

        if let Some(kind) = plan.required_property_query_kinds.iter().next() {
            return Err(SetupError::SetupFailed(format!(
                "scheme '{}' does not support property query {:?} for table {} col {}",
                self.name(),
                kind,
                plan.table_id.0,
                plan.col_id.0,
            )));
        }

        #[cfg(feature = "prove")]
        {
            Ok(ColumnViews::new(
                Arc::new(SmtRuntimeColumn { plan: plan.clone() }),
                Arc::new(SmtProofColumn::<W> { plan: plan.clone() }),
                Arc::new(SmtTransitionBackend::<W>::new(plan)?),
            ))
        }

        #[cfg(not(feature = "prove"))]
        {
            Ok(ColumnViews::new(
                Arc::new(SmtRuntimeColumn { plan: plan.clone() }),
                Arc::new(SmtProofColumn::<W> { plan }),
            ))
        }
    }
}

#[derive(Debug)]
struct SmtRuntimeColumn {
    plan: ColumnPlan,
}

impl RuntimeColumn for SmtRuntimeColumn {
    fn name(&self) -> &str {
        "smt"
    }

    fn resolve_property(
        &self,
        query: &PropertyQuery,
        _state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, tabula_core::error::TabulaError> {
        Err(tabula_core::error::TabulaError::InvalidIr(format!(
            "column scheme '{}' does not implement property query {:?} for table {} col {}",
            self.name(),
            query.kind(),
            self.plan.table_id.0,
            self.plan.col_id.0,
        )))
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
    }
}

#[derive(Debug)]
struct SmtProofColumn<const W: usize> {
    plan: ColumnPlan,
}

impl<const W: usize> ProofColumn for SmtProofColumn<W> {
    fn name(&self) -> &str {
        "smt"
    }

    fn table_id(&self) -> tabula_core::TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> tabula_core::ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn create_chips(&self, alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        let t = self.plan.table_id.0;
        let c = self.plan.col_id.0;

        let mem_id = alloc.next();
        let state_id = alloc.next();
        let meta_id = alloc.next();

        let mem = MemoryShardChip::<W>::new(mem_id, t, c);
        let state = SmtStateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            scheme_tags::SMT,
            self.plan.receives_commitment,
        );

        Ok(ColumnChipSet {
            airs: vec![
                Box::new(mem.clone()) as Box<dyn AnyRap>,
                Box::new(state.clone()),
                Box::new(meta.clone()),
            ],
            dyn_chips: vec![
                Box::new(mem) as Box<dyn DynChip>,
                Box::new(state),
                Box::new(meta),
            ],
            bus_consumers: vec![],
        })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SmtTransitionBackend<const W: usize> {
    plan: ColumnPlan,
}

#[cfg(feature = "prove")]
impl<const W: usize> SmtTransitionBackend<W> {
    fn new(plan: ColumnPlan) -> Result<Self, SetupError> {
        if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SMT_V1 {
            return Err(SetupError::SetupFailed(format!(
                "SMT transition backend cannot prepare column layout {}",
                plan.scheme_descriptor.layout_kind.0,
            )));
        }
        Ok(Self { plan })
    }
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnTransitionBackend for SmtTransitionBackend<W> {
    fn name(&self) -> &str {
        "smt"
    }

    fn table_id(&self) -> tabula_core::TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> tabula_core::ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn build_proof_input(
        &self,
        input: ColumnTransitionInput,
        property_reads: &[tabula_witness::trace::builtin::PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        if !property_reads.is_empty() {
            return Err(TabulaError::ProofError {
                phase: "smt_transition",
                detail: format!(
                    "SMT column ({}, {}) received unexpected property reads",
                    input.table.0, input.col.0
                ),
            });
        }

        let hasher = PoseidonHasher::new();
        let (old_state, _) = ColumnState::commit(
            &hasher,
            input.table,
            input.col,
            input.old_entries,
            scheme_tags::SMT,
        )?;
        let com_old = proof_column_commitment(input.table, input.col, &old_state)?;
        let is_empty_old = old_state.is_empty();
        let (new_state, _runtime_com_new, _merge_trace) = if input.is_touched {
            old_state.apply_writes(&hasher, input.table, input.col, &input.writes)
        } else {
            (old_state.clone(), com_old, None)
        };
        let meta = ColumnMeta {
            table: input.table,
            col: input.col,
            tag: scheme_tags::SMT,
            com_old,
            com_new: proof_column_commitment(input.table, input.col, &new_state)?,
            is_empty_old,
            is_empty_new: new_state.is_empty(),
            is_touched: input.is_touched,
        };

        let memory_rows = prepare_memory_shard_rows_from_parts::<W>(
            input.table,
            input.col,
            &input.init_rows,
            &input.access_rows,
        )?;
        let meta_row = prepare_meta_shard_row_from_parts(&meta, &input.access_rows, true);
        let shared = SharedColumnWitness {
            memory_rows,
            meta_row: (meta_row.is_touched || meta_row.empty_read_count > 0).then_some(meta_row),
        };

        let state_witness = build_smt_state_witness::<W>(
            (input.table, input.col),
            &input.init_rows,
            &input.access_rows,
            meta.is_touched,
            &meta,
            &old_state,
            &new_state,
        )?;

        let mut store = WitnessStore::new();
        store.put(SHARED_COLUMN_WITNESS_LABEL, shared);
        store.put(SMT_STATE_WITNESS_LABEL, state_witness);

        Ok(ColumnProofInput {
            table: input.table,
            col: input.col,
            meta,
            witness_store: store,
        })
    }
}

#[cfg(feature = "prove")]
fn build_smt_state_witness<const W: usize>(
    column: (tabula_core::TableId, tabula_core::ColId),
    init_rows: &[tabula_witness::InitRow],
    access_rows: &[tabula_witness::AccessRow],
    is_touched: bool,
    meta: &ColumnMeta,
    old_state: &ColumnState<PoseidonHasher>,
    new_state: &ColumnState<PoseidonHasher>,
) -> Result<SmtStateWitness<W>, TabulaError> {
    let (table, col) = column;
    let ColumnState::Smt(old_tree) = old_state else {
        return Err(TabulaError::ProofError {
            phase: "smt_transition",
            detail: format!(
                "column ({}, {}) is not SMT-backed in old state",
                table.0, col.0
            ),
        });
    };
    let ColumnState::Smt(new_tree) = new_state else {
        return Err(TabulaError::ProofError {
            phase: "smt_transition",
            detail: format!(
                "column ({}, {}) is not SMT-backed in new state",
                table.0, col.0
            ),
        });
    };

    let init_by_key = collect_init_rows::<W>(table, col, init_rows)?;
    let writes_by_key = collect_final_writes::<W>(table, col, access_rows)?;

    let mut keys: BTreeSet<_> = init_by_key.keys().copied().collect();
    keys.extend(writes_by_key.keys().copied());

    if is_touched && keys.is_empty() {
        return Err(TabulaError::ProofError {
            phase: "smt_transition",
            detail: format!(
                "touched SMT column ({}, {}) has no touched keys",
                table.0, col.0,
            ),
        });
    }

    let hasher = PoseidonHasher::new();
    let empty_leaf = hasher.hash_domain(tabula_commitment::DOMAIN_SMT, &[]);

    let mut paths = Vec::with_capacity(keys.len());
    for key in keys {
        if key.0 >= (1u64 << COL_DATA_SMT_DEPTH) {
            return Err(TabulaError::ProofError {
                phase: "smt_transition",
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

        let old_proof = old_tree.prove(key.0);
        let new_proof = new_tree.prove(key.0);

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
        column_old_root: meta.com_old,
        column_new_root: meta.com_new,
        column_is_empty_old: meta.is_empty_old,
        column_is_empty_new: meta.is_empty_new,
        column_is_touched: meta.is_touched,
        paths,
    })
}

#[cfg(feature = "prove")]
fn collect_init_rows<const W: usize>(
    table: tabula_core::TableId,
    col: tabula_core::ColId,
    init_rows: &[tabula_witness::InitRow],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    init_rows
        .iter()
        .map(|row| {
            let value: [KoalaBear; W] = row.value_fes.clone().try_into().map_err(|_| {
                TabulaError::ProofError {
                    phase: "smt_transition",
                    detail: format!(
                        "init row width mismatch for table {} col {} key {}: expected {}, got {}",
                        table.0,
                        col.0,
                        row.key.row.0,
                        W,
                        row.value_fes.len(),
                    ),
                }
            })?;
            Ok((row.key.row, (value, row.val_is_null)))
        })
        .collect()
}

#[cfg(feature = "prove")]
fn collect_final_writes<const W: usize>(
    table: tabula_core::TableId,
    col: tabula_core::ColId,
    access_rows: &[tabula_witness::AccessRow],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    let mut writes = BTreeMap::new();
    for access in access_rows {
        if !access.is_write {
            continue;
        }
        let value: [KoalaBear; W] =
            access
                .value_fes
                .clone()
                .try_into()
                .map_err(|_| TabulaError::ProofError {
                    phase: "smt_transition",
                    detail: format!(
                        "write row width mismatch for table {} col {} key {}: expected {}, got {}",
                        table.0,
                        col.0,
                        access.key.row.0,
                        W,
                        access.value_fes.len(),
                    ),
                })?;
        writes.insert(access.key.row, (value, access.val_is_null));
    }
    Ok(writes)
}

#[cfg(feature = "prove")]
fn validate_leaf_match<const W: usize>(
    phase: &str,
    key: RowKey,
    proof_value: &Option<tabula_commitment::NativeDigest>,
    value: &[KoalaBear; W],
    is_null: bool,
    hasher: &PoseidonHasher,
    empty_leaf: tabula_commitment::NativeDigest,
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
            phase: "smt_transition",
            detail: format!(
                "{phase} SMT proof/value mismatch for key {}: expected_null={} got={other:?}",
                key.0, is_null,
            ),
        }),
    }
}

#[cfg(feature = "prove")]
fn path_bits_from_key(key: u64) -> Vec<bool> {
    (0..COL_DATA_SMT_DEPTH)
        .map(|i| ((key >> i) & 1) == 1)
        .collect()
}
