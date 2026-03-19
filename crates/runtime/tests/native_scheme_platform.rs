//! End-to-end validation of the final native column scheme platform.
#![cfg(feature = "prove")]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_artifact::{
    ProgramArtifact, SchemeDescriptor, StateEntry, StateSnapshot, TransactionBatch,
    TransactionInput,
};
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::SsmcPropertyChip;
use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::{
    SMT_STATE_WITNESS_LABEL, SmtStatePathWitness, SmtStateShardChip, SmtStateWitness,
};
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_chips::shards::state::StateShardChip;
use tabula_commitment::{
    COL_DATA_SMT_DEPTH, ColumnMeta, ColumnState, FieldHasher, NativeDigest, PoseidonHasher,
    scheme_tags,
};
use tabula_compiler::{
    SchemeDescriptorCatalog, compile_program_source, register_program, register_program_artifact,
    register_program_definition_with_scheme_catalog,
};
use tabula_core::error::TabulaError;
use tabula_core::{
    CellKey, ColId, ColumnDef, ColumnLayoutKind, PropertyQueryResult, RootProfileId, RowKey,
    SchemeId, TableId, TableSchema, TxTypeId, Value, ValueType, zero_value,
};
use tabula_ir::{Instruction, PropertyQuery, PropertyQueryKind, TxTypeDef};
use tabula_machine::prelude::{ChipIdAllocator, DynChip};
use tabula_machine::{AnyRap, ColumnChipSet, ProofColumn, SetupError};
use tabula_runtime::{
    ColumnPlan, ColumnProofInput, ColumnSchemeFactory, ColumnTransitionBackend,
    ColumnTransitionInput, ColumnViews, ProgramVerifier, ProveInput, RuntimeColumn, TabulaRuntime,
};
use tabula_stark::air::interaction::BusId;
use tabula_stark::debug::RecordedInteraction;
use tabula_stark::trace::BusConsumer;
use tabula_stark::trace::WitnessStore;
use tabula_witness::trace::builtin::PropertyReadRecord as BuiltinPropertyReadRecord;
use tabula_witness::trace::builtin::memory::{
    prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,
    prepare_ssmc_column_witness_from_parts,
};
use tabula_witness::{AccessRow, InitRow, proof_column_commitment};

const INDEXED_SCHEME_ID: SchemeId = SchemeId(0x4301);
const ORDERBOOK_SCHEME_ID: SchemeId = SchemeId(0x4302);
const FRI_SCHEME_ID: SchemeId = SchemeId(0x4303);

const INDEXED_LAYOUT: ColumnLayoutKind = ColumnLayoutKind(0x9101);
const ORDERBOOK_LAYOUT: ColumnLayoutKind = ColumnLayoutKind(0x9102);
const FRI_LAYOUT: ColumnLayoutKind = ColumnLayoutKind(0x9103);

const INDEXED_TAG: u16 = 101;
const ORDERBOOK_TAG: u16 = 102;
const FRI_TAG: u16 = 103;

#[derive(Clone)]
struct SchemeProfile {
    descriptor: SchemeDescriptor,
    name: &'static str,
    tag: u16,
    extra_bus_consumer: bool,
}

fn profile(
    scheme_id: SchemeId,
    layout_kind: ColumnLayoutKind,
    name: &'static str,
    tag: u16,
    supported_property_query_kinds: Vec<PropertyQueryKind>,
    extra_bus_consumer: bool,
    params_seed: u8,
) -> SchemeProfile {
    SchemeProfile {
        descriptor: SchemeDescriptor {
            scheme_id,
            scheme_version: 1,
            layout_kind,
            params_hash: [params_seed; 32],
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds,
        },
        name,
        tag,
        extra_bus_consumer,
    }
}

fn single_column_state(value: u64) -> StateSnapshot {
    StateSnapshot {
        cells: vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(Value::U64(value)),
        }],
    }
}

fn single_tx_batch(amount: u64) -> TransactionBatch {
    TransactionBatch {
        transactions: vec![TransactionInput {
            tx_type: 0,
            params: vec![Value::U64(amount)],
            sender: "01".repeat(32),
            nonce: 0,
        }],
    }
}

fn no_param_batch() -> TransactionBatch {
    TransactionBatch {
        transactions: vec![TransactionInput {
            tx_type: 1,
            params: vec![],
            sender: "01".repeat(32),
            nonce: 0,
        }],
    }
}

fn source_artifact_for_scheme(
    scheme_id: SchemeId,
    descriptor: &SchemeDescriptor,
) -> ProgramArtifact {
    let source = format!(
        "\
table balances {{
    amount: u64 @scheme({})
}}

tx bump(amount: u64) {{
    let current = balances[0].amount
    balances[0].amount = current + amount
}}
",
        scheme_id.0
    );
    let definition = compile_program_source(&source).expect("compile source");
    let mut catalog = SchemeDescriptorCatalog::new();
    catalog.insert(scheme_id, descriptor.clone());
    register_program_definition_with_scheme_catalog(&definition, &catalog)
        .expect("register custom scheme source")
        .into_program_artifact()
}

fn orderbook_artifact(descriptor: &SchemeDescriptor) -> ProgramArtifact {
    let schema = TableSchema {
        id: TableId(0),
        name: "orders".to_string(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "qty".to_string(),
            value_type: ValueType::U64,
        }],
    };
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "scan".to_string(),
        param_schema: vec![],
        body: vec![Instruction::PropertyRead {
            dst_val: 0,
            dst_key: 1,
            dst_is_null: 2,
            table: TableId(0),
            col: ColId(0),
            query: PropertyQuery::Successor { key: RowKey(0) },
        }],
    };

    let mut artifact = register_program(&[schema], &[tx])
        .expect("register orderbook program")
        .into_program_artifact();
    artifact.column_proof_plan[0].scheme_id = descriptor.scheme_id;
    artifact.column_proof_plan[0].scheme_descriptor = descriptor.clone();
    artifact
}

#[derive(Clone)]
struct SparseNativeScheme<const W: usize> {
    profile: SchemeProfile,
}

impl<const W: usize> SparseNativeScheme<W> {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl<const W: usize> ColumnSchemeFactory for SparseNativeScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_id != self.profile.descriptor.scheme_id {
            return Err(SetupError::SetupFailed(format!(
                "{} expected scheme id {} but received {}",
                self.profile.name, self.profile.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        if plan.scheme_descriptor.layout_kind != self.profile.descriptor.layout_kind {
            return Err(SetupError::SetupFailed(format!(
                "{} expected layout {} but received {}",
                self.profile.name,
                self.profile.descriptor.layout_kind.0,
                plan.scheme_descriptor.layout_kind.0,
            )));
        }
        if let Some(kind) = plan.required_property_query_kinds.iter().next() {
            return Err(SetupError::SetupFailed(format!(
                "{} does not support property query {:?} for table {} col {}",
                self.profile.name, kind, plan.table_id.0, plan.col_id.0,
            )));
        }

        Ok(ColumnViews::new(
            Arc::new(SparseRuntimeColumn {
                profile: self.profile.clone(),
                plan: plan.clone(),
            }),
            Arc::new(SparseProofColumn::<W> {
                profile: self.profile.clone(),
                plan: plan.clone(),
            }),
            Arc::new(SparseTransitionBackend::<W> {
                profile: self.profile.clone(),
                plan,
            }),
        ))
    }
}

#[derive(Clone)]
struct OrderedNativeScheme<const W: usize> {
    profile: SchemeProfile,
}

impl<const W: usize> OrderedNativeScheme<W> {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl<const W: usize> ColumnSchemeFactory for OrderedNativeScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_id != self.profile.descriptor.scheme_id {
            return Err(SetupError::SetupFailed(format!(
                "{} expected scheme id {} but received {}",
                self.profile.name, self.profile.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        if plan.scheme_descriptor.layout_kind != self.profile.descriptor.layout_kind {
            return Err(SetupError::SetupFailed(format!(
                "{} expected layout {} but received {}",
                self.profile.name,
                self.profile.descriptor.layout_kind.0,
                plan.scheme_descriptor.layout_kind.0,
            )));
        }
        if let Some(kind) = plan.required_property_query_kinds.iter().find(|kind| {
            !self
                .profile
                .descriptor
                .supported_property_query_kinds
                .contains(kind)
        }) {
            return Err(SetupError::SetupFailed(format!(
                "{} does not support property query {:?} for table {} col {}",
                self.profile.name, kind, plan.table_id.0, plan.col_id.0,
            )));
        }

        Ok(ColumnViews::new(
            Arc::new(OrderedRuntimeColumn {
                profile: self.profile.clone(),
                plan: plan.clone(),
            }),
            Arc::new(OrderedProofColumn::<W> {
                profile: self.profile.clone(),
                plan: plan.clone(),
            }),
            Arc::new(OrderedTransitionBackend::<W> {
                profile: self.profile.clone(),
                plan,
            }),
        ))
    }
}

#[derive(Clone)]
struct SparseRuntimeColumn {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl RuntimeColumn for SparseRuntimeColumn {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
    }

    fn resolve_property(
        &self,
        query: &PropertyQuery,
        _state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, TabulaError> {
        Err(TabulaError::InvalidIr(format!(
            "column scheme '{}' does not implement property query {:?} for table {} col {}",
            self.profile.name,
            query.kind(),
            self.plan.table_id.0,
            self.plan.col_id.0,
        )))
    }
}

#[derive(Clone)]
struct OrderedRuntimeColumn {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl RuntimeColumn for OrderedRuntimeColumn {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &self.profile.descriptor.supported_property_query_kinds
    }

    fn resolve_property(
        &self,
        query: &PropertyQuery,
        state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, TabulaError> {
        let non_null = || state.iter().filter(|(_, _, is_null)| !*is_null);

        let resolved = match query {
            PropertyQuery::Successor { key } => non_null()
                .filter(|(candidate, _, _)| *candidate > *key)
                .min_by_key(|(candidate, _, _)| *candidate)
                .map(|(candidate, value, _)| PropertyQueryResult {
                    value: *value,
                    key: Some(*candidate),
                    is_null: false,
                }),
            PropertyQuery::Predecessor { key } => non_null()
                .filter(|(candidate, _, _)| *candidate < *key)
                .max_by_key(|(candidate, _, _)| *candidate)
                .map(|(candidate, value, _)| PropertyQueryResult {
                    value: *value,
                    key: Some(*candidate),
                    is_null: false,
                }),
            other => {
                return Err(TabulaError::InvalidIr(format!(
                    "column scheme '{}' does not implement property query {:?} for table {} col {}",
                    self.profile.name,
                    other.kind(),
                    self.plan.table_id.0,
                    self.plan.col_id.0,
                )));
            }
        };

        Ok(resolved.unwrap_or(PropertyQueryResult {
            value: zero_value(self.plan.value_type),
            key: None,
            is_null: true,
        }))
    }
}

struct NoopConsumer;

impl BusConsumer for NoopConsumer {
    fn consumed_buses(&self) -> Vec<BusId> {
        vec![]
    }

    fn collect(
        &self,
        _interactions: &[RecordedInteraction<KoalaBear>],
        _store: &mut WitnessStore,
    ) -> Result<(), TabulaError> {
        Ok(())
    }
}

#[derive(Clone)]
struct SparseProofColumn<const W: usize> {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl<const W: usize> ProofColumn for SparseProofColumn<W> {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn table_id(&self) -> TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> ColId {
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
            self.profile.tag,
            self.plan.receives_commitment,
        );

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> = vec![];
        if self.profile.extra_bus_consumer {
            bus_consumers.push(Box::new(NoopConsumer));
        }

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
            bus_consumers,
        })
    }
}

#[derive(Clone)]
struct OrderedProofColumn<const W: usize> {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl<const W: usize> ProofColumn for OrderedProofColumn<W> {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn table_id(&self) -> TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> ColId {
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
        let state = StateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            self.profile.tag,
            self.plan.receives_commitment,
        );

        let mut airs: Vec<Box<dyn AnyRap>> = vec![
            Box::new(mem.clone()),
            Box::new(state.clone()),
            Box::new(meta.clone()),
        ];
        let mut dyn_chips: Vec<Box<dyn DynChip>> =
            vec![Box::new(mem), Box::new(state), Box::new(meta)];

        if self.plan.requires_property_support() {
            let prop_id = alloc.next();
            let prop = SsmcPropertyChip::<W>::new(prop_id, t, c);
            airs.push(Box::new(prop.clone()));
            dyn_chips.push(Box::new(prop));
        }

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> = vec![];
        if self.profile.extra_bus_consumer {
            bus_consumers.push(Box::new(NoopConsumer));
        }

        Ok(ColumnChipSet {
            airs,
            dyn_chips,
            bus_consumers,
        })
    }
}

#[derive(Clone)]
struct SparseTransitionBackend<const W: usize> {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl<const W: usize> ColumnTransitionBackend for SparseTransitionBackend<W> {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn table_id(&self) -> TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn build_proof_input(
        &self,
        input: ColumnTransitionInput,
        property_reads: &[BuiltinPropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        if !property_reads.is_empty() {
            return Err(TabulaError::ProofError {
                phase: self.profile.name,
                detail: format!(
                    "{} column ({}, {}) received unexpected property reads",
                    self.profile.name, input.table.0, input.col.0
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
            tag: self.profile.tag,
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

        let state_witness = build_sparse_state_witness::<W>(
            &self.profile,
            (input.table, input.col),
            &input.init_rows,
            &input.access_rows,
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

#[derive(Clone)]
struct OrderedTransitionBackend<const W: usize> {
    profile: SchemeProfile,
    plan: ColumnPlan,
}

impl<const W: usize> ColumnTransitionBackend for OrderedTransitionBackend<W> {
    fn name(&self) -> &str {
        self.profile.name
    }

    fn table_id(&self) -> TableId {
        self.plan.table_id
    }

    fn col_id(&self) -> ColId {
        self.plan.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn build_proof_input(
        &self,
        input: ColumnTransitionInput,
        property_reads: &[PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        let hasher = PoseidonHasher::new();
        let (old_state, _) = ColumnState::commit(
            &hasher,
            input.table,
            input.col,
            input.old_entries,
            scheme_tags::SSMC,
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
            tag: self.profile.tag,
            com_old,
            com_new: proof_column_commitment(input.table, input.col, &new_state)?,
            is_empty_old,
            is_empty_new: new_state.is_empty(),
            is_touched: input.is_touched,
        };
        let init_rows = if property_reads.is_empty() {
            input.init_rows.clone()
        } else {
            synthesize_old_init_rows(input.table, input.col, &old_state)?
        };

        let column_witness = prepare_ssmc_column_witness_from_parts::<W>(
            (input.table, input.col),
            &init_rows,
            &input.access_rows,
            &ordered_entries(&old_state)?,
            &ordered_entries(&new_state)?,
            &meta,
            true,
        )?;

        let mut store = WitnessStore::new();
        store.put(
            SHARED_COLUMN_WITNESS_LABEL,
            SharedColumnWitness {
                memory_rows: column_witness.memory_rows.clone(),
                meta_row: column_witness.meta_row.clone(),
            },
        );
        let mut single = SsmcWitness::default();
        single.insert(self.plan.table_id, self.plan.col_id, column_witness);
        store.put(SSMC_WITNESS_LABEL, single);
        if !property_reads.is_empty() {
            store.put(PROPERTY_READ_WITNESS_LABEL, property_reads.to_vec());
        }

        Ok(ColumnProofInput {
            table: input.table,
            col: input.col,
            meta,
            witness_store: store,
        })
    }
}

fn ordered_entries(
    state: &ColumnState<PoseidonHasher>,
) -> Result<BTreeMap<RowKey, Vec<KoalaBear>>, TabulaError> {
    match state {
        ColumnState::Ssmc(list) => Ok(list
            .entries()
            .iter()
            .map(|entry| (entry.key, entry.value.clone()))
            .collect()),
        ColumnState::Smt(_) => Err(TabulaError::ProofError {
            phase: "ordered_test_backend",
            detail: "expected ordered SSMC-style state".to_string(),
        }),
    }
}

fn synthesize_old_init_rows(
    table: TableId,
    col: ColId,
    state: &ColumnState<PoseidonHasher>,
) -> Result<Vec<InitRow>, TabulaError> {
    Ok(ordered_entries(state)?
        .into_iter()
        .map(|(row, value_fes)| InitRow {
            key: CellKey { table, col, row },
            value_fes,
            val_is_null: false,
        })
        .collect())
}

fn build_sparse_state_witness<const W: usize>(
    profile: &SchemeProfile,
    column: (TableId, ColId),
    init_rows: &[InitRow],
    access_rows: &[AccessRow],
    meta: &ColumnMeta,
    old_state: &ColumnState<PoseidonHasher>,
    new_state: &ColumnState<PoseidonHasher>,
) -> Result<SmtStateWitness<W>, TabulaError> {
    let (table, col) = column;
    let ColumnState::Smt(old_tree) = old_state else {
        return Err(TabulaError::ProofError {
            phase: profile.name,
            detail: format!(
                "{} old state for ({}, {}) is not sparse-tree backed",
                profile.name, table.0, col.0
            ),
        });
    };
    let ColumnState::Smt(new_tree) = new_state else {
        return Err(TabulaError::ProofError {
            phase: profile.name,
            detail: format!(
                "{} new state for ({}, {}) is not sparse-tree backed",
                profile.name, table.0, col.0
            ),
        });
    };

    let init_by_key = collect_init_rows::<W>(profile, table, col, init_rows)?;
    let writes_by_key = collect_final_writes::<W>(profile, table, col, access_rows)?;

    let mut keys: BTreeSet<_> = init_by_key.keys().copied().collect();
    keys.extend(writes_by_key.keys().copied());

    if meta.is_touched && keys.is_empty() {
        return Err(TabulaError::ProofError {
            phase: profile.name,
            detail: format!(
                "{} touched sparse column ({}, {}) has no touched keys",
                profile.name, table.0, col.0
            ),
        });
    }

    let hasher = PoseidonHasher::new();
    let empty_leaf = hasher.hash_domain(tabula_commitment::DOMAIN_SMT, &[]);

    let mut paths = Vec::with_capacity(keys.len());
    for key in keys {
        if key.0 >= (1u64 << COL_DATA_SMT_DEPTH) {
            return Err(TabulaError::ProofError {
                phase: profile.name,
                detail: format!(
                    "{} key {} exceeds sparse depth {}",
                    profile.name, key.0, COL_DATA_SMT_DEPTH
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
            profile,
            ("old", key),
            &old_proof.value,
            &old_val,
            old_is_null,
            &hasher,
            empty_leaf,
        )?;
        validate_leaf_match(
            profile,
            ("new", key),
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

fn collect_init_rows<const W: usize>(
    profile: &SchemeProfile,
    table: TableId,
    col: ColId,
    init_rows: &[InitRow],
) -> Result<BTreeMap<RowKey, ([KoalaBear; W], bool)>, TabulaError> {
    init_rows
        .iter()
        .map(|row| {
            let value: [KoalaBear; W] =
                row.value_fes
                    .clone()
                    .try_into()
                    .map_err(|_| TabulaError::ProofError {
                        phase: profile.name,
                        detail: format!(
                            "{} init row width mismatch for ({}, {}, {}): expected {}, got {}",
                            profile.name,
                            table.0,
                            col.0,
                            row.key.row.0,
                            W,
                            row.value_fes.len(),
                        ),
                    })?;
            Ok((row.key.row, (value, row.val_is_null)))
        })
        .collect()
}

fn collect_final_writes<const W: usize>(
    profile: &SchemeProfile,
    table: TableId,
    col: ColId,
    access_rows: &[AccessRow],
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
                    phase: profile.name,
                    detail: format!(
                        "{} write row width mismatch for ({}, {}, {}): expected {}, got {}",
                        profile.name,
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

fn validate_leaf_match<const W: usize>(
    profile: &SchemeProfile,
    target: (&str, RowKey),
    proof_value: &Option<NativeDigest>,
    value: &[KoalaBear; W],
    is_null: bool,
    hasher: &PoseidonHasher,
    empty_leaf: NativeDigest,
) -> Result<(), TabulaError> {
    let (phase, key) = target;
    let expected = if is_null {
        empty_leaf
    } else {
        hasher.hash(value)
    };
    match proof_value {
        Some(digest) if !is_null && *digest == expected => Ok(()),
        None if is_null => Ok(()),
        other => Err(TabulaError::ProofError {
            phase: profile.name,
            detail: format!(
                "{} {phase} sparse proof/value mismatch for key {}: expected_null={} got={other:?}",
                profile.name, key.0, is_null
            ),
        }),
    }
}

fn path_bits_from_key(key: u64) -> Vec<bool> {
    (0..COL_DATA_SMT_DEPTH)
        .map(|i| ((key >> i) & 1) == 1)
        .collect()
}

#[test]
fn indexed_merkle_like_scheme_flows_from_source_catalog_and_public_seam() {
    let profile = profile(
        INDEXED_SCHEME_ID,
        INDEXED_LAYOUT,
        "indexed_merkle_like",
        INDEXED_TAG,
        vec![],
        false,
        0x31,
    );
    let artifact = source_artifact_for_scheme(profile.descriptor.scheme_id, &profile.descriptor);
    assert_eq!(
        artifact.column_proof_plan[0].scheme_descriptor.layout_kind,
        INDEXED_LAYOUT
    );

    let compiled = register_program_artifact(&artifact).expect("compiled program");
    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme(SparseNativeScheme::<3>::new(profile.clone()))
        .expect("register indexed-merkle-like scheme")
        .build()
        .expect("runtime");

    let state = single_column_state(10);
    let batch = single_tx_batch(5);
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .with_scheme(SparseNativeScheme::<3>::new(profile))
        .expect("register indexed-merkle-like verifier scheme")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("indexed-merkle-like verifier succeeds");
}

#[test]
fn orderbook_like_scheme_proves_structural_property_reads_via_public_seam() {
    let profile = profile(
        ORDERBOOK_SCHEME_ID,
        ORDERBOOK_LAYOUT,
        "orderbook_like",
        ORDERBOOK_TAG,
        vec![PropertyQueryKind::Successor, PropertyQueryKind::Predecessor],
        false,
        0x32,
    );
    let artifact = orderbook_artifact(&profile.descriptor);
    assert_eq!(
        artifact.column_proof_plan[0].scheme_descriptor.layout_kind,
        ORDERBOOK_LAYOUT
    );

    let compiled = register_program_artifact(&artifact).expect("compiled program");
    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme(OrderedNativeScheme::<3>::new(profile.clone()))
        .expect("register orderbook-like scheme")
        .build()
        .expect("runtime");

    let state = StateSnapshot {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(10)),
            },
            StateEntry {
                table: 0,
                row: 5,
                col: 0,
                value: Some(Value::U64(20)),
            },
        ],
    };
    let batch = no_param_batch();
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .with_scheme(OrderedNativeScheme::<3>::new(profile))
        .expect("register orderbook-like verifier scheme")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("orderbook-like verifier succeeds");
}

#[test]
fn merkle_fri_like_scheme_accepts_extra_bus_consumer_without_shared_path_changes() {
    let profile = profile(
        FRI_SCHEME_ID,
        FRI_LAYOUT,
        "merkle_fri_like",
        FRI_TAG,
        vec![],
        true,
        0x33,
    );
    let artifact = source_artifact_for_scheme(profile.descriptor.scheme_id, &profile.descriptor);
    let compiled = register_program_artifact(&artifact).expect("compiled program");

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme(SparseNativeScheme::<3>::new(profile.clone()))
        .expect("register merkle-fri-like scheme")
        .build()
        .expect("runtime");

    let state = single_column_state(7);
    let batch = single_tx_batch(8);
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .with_scheme(SparseNativeScheme::<3>::new(profile))
        .expect("register merkle-fri-like verifier scheme")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("merkle-fri-like verifier succeeds");
}
