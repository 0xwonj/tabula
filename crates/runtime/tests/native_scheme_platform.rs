//! End-to-end validation of the split stable/proof-extension native scheme platform.
#![cfg(feature = "prove")]

use std::sync::Arc;

use p3_koala_bear::KoalaBear;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tabula_artifact::{Artifact, SchemeDescriptor, State, StateEntry};
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::smt_state::SmtStateShardChip;
use tabula_commitment::schemes::tags;
use tabula_compiler::{SchemeDescriptorCatalog, register_program};
use tabula_core::{
    ColId, ColumnDef, ColumnLayoutKind, PropertyQueryResult, RootProfileId, RowKey, SchemeId,
    TableId, TableSchema, TxTypeId, Value, ValueType, zero_value,
};
use tabula_ext::backend::scheme::{ColumnProofPreparer, ProofSchemeFactory};
use tabula_ext::backend::{
    AnyRap, BusConsumer, ChipIdAllocator, ColumnChipSet, ProofColumn, SetupError,
};
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn, SchemeBundle};
use tabula_ir::{Instruction, PropertyQuery, PropertyQueryKind, TxTypeDef};
use tabula_runtime::{ProveInput, SmtScheme, SsmcScheme, TabulaRuntime, Verifier};
use tabula_stark::air::interaction::BusId;
use tabula_stark::debug::RecordedInteraction;
use tabula_testing::exec::{artifact_from_source_with_catalog, compiled_program_from_artifact};
use tabula_testing::fixtures::batch::{no_param_batch, single_tx_batch};
use tabula_testing::fixtures::state::single_cell_u64;

const INDEXED_SCHEME_ID: SchemeId = SchemeId(0x4301);
const ORDERBOOK_SCHEME_ID: SchemeId = SchemeId(0x4302);
const FRI_SCHEME_ID: SchemeId = SchemeId(0x4303);

const INDEXED_LAYOUT: ColumnLayoutKind = ColumnLayoutKind::SMT_V1;
const ORDERBOOK_LAYOUT: ColumnLayoutKind = ColumnLayoutKind::SSMC_V1;
const FRI_LAYOUT: ColumnLayoutKind = ColumnLayoutKind::SMT_V1;
const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";

#[derive(Clone)]
struct SchemeProfile {
    descriptor: SchemeDescriptor,
    name: &'static str,
    extra_bus_consumer: bool,
}

fn profile(
    scheme_id: SchemeId,
    layout_kind: ColumnLayoutKind,
    name: &'static str,
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
        extra_bus_consumer,
    }
}

fn source_artifact_for_scheme(scheme_id: SchemeId, descriptor: &SchemeDescriptor) -> Artifact {
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
    let mut catalog = SchemeDescriptorCatalog::new();
    catalog.insert(scheme_id, descriptor.clone());
    artifact_from_source_with_catalog(&source, &catalog)
}

fn orderbook_artifact(descriptor: &SchemeDescriptor) -> Artifact {
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
        .into_artifact();
    artifact.column_proof_plan[0].scheme_id = descriptor.scheme_id;
    artifact.column_proof_plan[0].scheme_descriptor = descriptor.clone();
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.column_proof_plan,
        )
        .expect("semantic hash"),
    );
    artifact
}

fn compute_semantic_hash_stub(
    precompile_manifest: &[tabula_artifact::PrecompileDescriptor],
    required_property_requirements: &[tabula_ir::PropertyRequirement],
    column_proof_plan: &[tabula_artifact::ColumnProofPlan],
) -> Result<[u8; 32], serde_json::Error> {
    #[derive(Serialize)]
    struct SemanticContract<'a> {
        precompile_manifest: &'a [tabula_artifact::PrecompileDescriptor],
        required_property_requirements: &'a [tabula_ir::PropertyRequirement],
        column_proof_plan: &'a [tabula_artifact::ColumnProofPlan],
    }

    let payload = serde_json::to_vec(&SemanticContract {
        precompile_manifest,
        required_property_requirements,
        column_proof_plan,
    })?;

    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_HASH_DOMAIN);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    Ok(hasher.finalize().into())
}

#[derive(Clone)]
struct SparseNativeRuntimeScheme {
    profile: SchemeProfile,
}

impl SparseNativeRuntimeScheme {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl ColumnSchemeFactory for SparseNativeRuntimeScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        validate_sparse_plan(&self.profile, plan)?;
        SmtScheme::<3>.build_runtime_column(plan)
    }
}

#[derive(Clone)]
struct OrderedNativeRuntimeScheme {
    profile: SchemeProfile,
}

impl OrderedNativeRuntimeScheme {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl ColumnSchemeFactory for OrderedNativeRuntimeScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        validate_ordered_plan(&self.profile, plan)?;
        SsmcScheme::<3>.build_runtime_column(plan)
    }
}

#[derive(Clone)]
struct SparseNativeProofScheme {
    profile: SchemeProfile,
}

impl SparseNativeProofScheme {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl ProofSchemeFactory for SparseNativeProofScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        validate_sparse_plan(&self.profile, plan)?;
        if self.profile.extra_bus_consumer {
            Ok(Arc::new(SparseProofColumn {
                profile: self.profile.clone(),
                plan: plan.clone(),
            }))
        } else {
            SmtScheme::<3>.build_proof_column(plan)
        }
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        validate_sparse_plan(&self.profile, plan)?;
        SmtScheme::<3>.build_proof_preparer(plan)
    }
}

#[derive(Clone)]
struct OrderedNativeProofScheme {
    profile: SchemeProfile,
}

impl OrderedNativeProofScheme {
    fn new(profile: SchemeProfile) -> Self {
        Self { profile }
    }
}

impl ProofSchemeFactory for OrderedNativeProofScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        self.profile.descriptor.clone()
    }

    fn name(&self) -> &str {
        self.profile.name
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        validate_ordered_plan(&self.profile, plan)?;
        SsmcScheme::<3>.build_proof_column(plan)
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        validate_ordered_plan(&self.profile, plan)?;
        SsmcScheme::<3>.build_proof_preparer(plan)
    }
}

fn sparse_native_bundle(profile: SchemeProfile) -> SchemeBundle {
    let runtime = SparseNativeRuntimeScheme::new(profile.clone());
    let proof = SparseNativeProofScheme::new(profile);
    SchemeBundle::new(runtime, proof).expect("sparse native bundle")
}

fn ordered_native_bundle(profile: SchemeProfile) -> SchemeBundle {
    let runtime = OrderedNativeRuntimeScheme::new(profile.clone());
    let proof = OrderedNativeProofScheme::new(profile);
    SchemeBundle::new(runtime, proof).expect("ordered native bundle")
}

fn validate_sparse_plan(
    profile: &SchemeProfile,
    plan: &ResolvedColumnPlan,
) -> Result<(), ExtError> {
    if plan.scheme_id != profile.descriptor.scheme_id {
        return Err(ExtError::validation(format!(
            "{} expected scheme id {} but received {}",
            profile.name, profile.descriptor.scheme_id.0, plan.scheme_id.0,
        )));
    }
    if plan.scheme_descriptor.layout_kind != profile.descriptor.layout_kind {
        return Err(ExtError::validation(format!(
            "{} expected layout {} but received {}",
            profile.name, profile.descriptor.layout_kind.0, plan.scheme_descriptor.layout_kind.0,
        )));
    }
    if let Some(kind) = plan.required_property_query_kinds.iter().next() {
        return Err(ExtError::validation(format!(
            "{} does not support property query {:?} for table {} col {}",
            profile.name, kind, plan.table_id.0, plan.col_id.0,
        )));
    }
    Ok(())
}

fn validate_ordered_plan(
    profile: &SchemeProfile,
    plan: &ResolvedColumnPlan,
) -> Result<(), ExtError> {
    if plan.scheme_id != profile.descriptor.scheme_id {
        return Err(ExtError::validation(format!(
            "{} expected scheme id {} but received {}",
            profile.name, profile.descriptor.scheme_id.0, plan.scheme_id.0,
        )));
    }
    if plan.scheme_descriptor.layout_kind != profile.descriptor.layout_kind {
        return Err(ExtError::validation(format!(
            "{} expected layout {} but received {}",
            profile.name, profile.descriptor.layout_kind.0, plan.scheme_descriptor.layout_kind.0,
        )));
    }
    if let Some(kind) = plan.required_property_query_kinds.iter().find(|kind| {
        !profile
            .descriptor
            .supported_property_query_kinds
            .contains(kind)
    }) {
        return Err(ExtError::validation(format!(
            "{} does not support property query {:?} for table {} col {}",
            profile.name, kind, plan.table_id.0, plan.col_id.0,
        )));
    }
    Ok(())
}

#[derive(Clone)]
struct SparseProofColumn {
    profile: SchemeProfile,
    plan: ResolvedColumnPlan,
}

impl ProofColumn for SparseProofColumn {
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

        let mem = MemoryShardChip::<3>::new(mem_id, t, c);
        let state = SmtStateShardChip::<3>::new(state_id, t, c);
        let meta = MetaShardChip::new(meta_id, t, c, tags::SMT, self.plan.receives_commitment);

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> = vec![];
        if self.profile.extra_bus_consumer {
            bus_consumers.push(Box::new(NoopConsumer {
                label: self.profile.name,
            }));
        }

        Ok(ColumnChipSet {
            airs: vec![
                Box::new(mem.clone()) as Box<dyn AnyRap>,
                Box::new(state.clone()),
                Box::new(meta.clone()),
            ],
            dyn_chips: vec![Box::new(mem), Box::new(state), Box::new(meta)],
            bus_consumers,
        })
    }
}

struct NoopConsumer {
    label: &'static str,
}

impl BusConsumer for NoopConsumer {
    fn consumed_buses(&self) -> Vec<BusId> {
        vec![]
    }

    fn collect(
        &self,
        _interactions: &[RecordedInteraction<KoalaBear>],
        _store: &mut tabula_stark::trace::WitnessStore,
    ) -> Result<(), tabula_core::error::TabulaError> {
        let _ = self.label;
        Ok(())
    }
}

#[test]
fn indexed_merkle_like_scheme_flows_from_source_catalog_and_split_seam() {
    let profile = profile(
        INDEXED_SCHEME_ID,
        INDEXED_LAYOUT,
        "indexed_merkle_like",
        vec![],
        false,
        0x31,
    );
    let artifact = source_artifact_for_scheme(profile.descriptor.scheme_id, &profile.descriptor);
    assert_eq!(
        artifact.column_proof_plan[0].scheme_descriptor.layout_kind,
        INDEXED_LAYOUT
    );

    let compiled = compiled_program_from_artifact(&artifact);
    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(sparse_native_bundle(profile.clone()))
        .expect("register indexed scheme bundle")
        .build()
        .expect("runtime");

    let state = single_cell_u64(TableId(0), ColId(0), RowKey(0), 10);
    let batch = single_tx_batch(0, vec![Value::U64(5)]);
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .with_scheme_bundle(sparse_native_bundle(profile))
        .expect("register indexed verifier scheme bundle")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("indexed verifier succeeds");
}

#[test]
fn orderbook_like_scheme_proves_structural_property_reads_via_split_seam() {
    let profile = profile(
        ORDERBOOK_SCHEME_ID,
        ORDERBOOK_LAYOUT,
        "orderbook_like",
        vec![PropertyQueryKind::Successor, PropertyQueryKind::Predecessor],
        false,
        0x32,
    );
    let artifact = orderbook_artifact(&profile.descriptor);
    assert_eq!(
        artifact.column_proof_plan[0].scheme_descriptor.layout_kind,
        ORDERBOOK_LAYOUT
    );

    let compiled = compiled_program_from_artifact(&artifact);
    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(ordered_native_bundle(profile.clone()))
        .expect("register orderbook scheme bundle")
        .build()
        .expect("runtime");

    let state = State {
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
    let batch = no_param_batch(1);
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .with_scheme_bundle(ordered_native_bundle(profile))
        .expect("register orderbook verifier scheme bundle")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("orderbook verifier succeeds");
}

#[test]
fn merkle_fri_like_scheme_accepts_extra_bus_consumer_without_public_seam_changes() {
    let profile = profile(
        FRI_SCHEME_ID,
        FRI_LAYOUT,
        "merkle_fri_like",
        vec![],
        true,
        0x33,
    );
    let artifact = source_artifact_for_scheme(profile.descriptor.scheme_id, &profile.descriptor);
    let compiled = compiled_program_from_artifact(&artifact);

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(sparse_native_bundle(profile.clone()))
        .expect("register merkle scheme bundle")
        .build()
        .expect("runtime");

    let state = single_cell_u64(TableId(0), ColId(0), RowKey(0), 7);
    let batch = single_tx_batch(0, vec![Value::U64(8)]);
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .with_scheme_bundle(sparse_native_bundle(profile))
        .expect("register merkle verifier scheme bundle")
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("merkle verifier succeeds");
}

#[test]
fn ordered_runtime_column_resolves_supported_property_queries() {
    let scheme_profile = profile(
        ORDERBOOK_SCHEME_ID,
        ORDERBOOK_LAYOUT,
        "orderbook_like",
        vec![PropertyQueryKind::Successor, PropertyQueryKind::Predecessor],
        false,
        0x32,
    );
    let runtime_column = OrderedNativeRuntimeScheme::new(scheme_profile.clone())
        .build_runtime_column(&ResolvedColumnPlan {
            table_id: TableId(0),
            col_id: ColId(0),
            scheme_id: ORDERBOOK_SCHEME_ID,
            scheme_descriptor: SchemeDescriptor {
                layout_kind: ORDERBOOK_LAYOUT,
                ..scheme_profile.descriptor
            },
            value_type: ValueType::U64,
            receives_commitment: true,
            required_property_query_kinds: [PropertyQueryKind::Successor].into_iter().collect(),
        })
        .expect("runtime column");

    let state = vec![
        (RowKey(5), Value::U64(50), false),
        (RowKey(10), Value::U64(100), false),
    ];
    let result = runtime_column
        .resolve_property(&PropertyQuery::Successor { key: RowKey(5) }, &state)
        .expect("successor");
    assert_eq!(
        result,
        PropertyQueryResult {
            value: Value::U64(100),
            key: Some(RowKey(10)),
            is_null: false,
        }
    );

    let missing = runtime_column
        .resolve_property(&PropertyQuery::Predecessor { key: RowKey(5) }, &state)
        .expect("missing predecessor");
    assert_eq!(missing.value, zero_value(ValueType::U64));
    assert!(missing.is_null);
}
