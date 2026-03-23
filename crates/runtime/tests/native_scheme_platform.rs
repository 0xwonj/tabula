//! End-to-end validation of the canonical native scheme platform.
#![cfg(feature = "prove")]

use tabula_artifact::State;
use tabula_core::{
    ColId, ColumnLayoutKind, RootProfileId, RowKey, SchemeId, SchemeProfileId, TableId,
};
use tabula_ext::{
    ColumnBackendFactory, ColumnBackendFactoryBundle, ColumnBackendSetup, ExtError,
    MaterializedColumnBackend,
};
use tabula_ir::{PropertyQuery, PropertyQueryKind};
use tabula_profile::{
    CanonicalNullEncoding, CommitmentContractKind, ENCODING_U64_ID, EncodingClass,
    EncodingRequirements, FieldFamily, SchemeProfile, SemanticRegistry, TranscriptSerialization,
    VerifierDigestFormat, WidthConstraint, builtin_semantic_registry,
};
use tabula_runtime::{
    HostEnvironment, ProveInput, RuntimeRegistries, SmtScheme, SsmcScheme, TabulaRuntime, Verifier,
};
use tabula_testing::exec::{artifact_from_source_with_registry, compiled_program_from_artifact};
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::state::single_cell_u64;
use tabula_types::{TypedColumnEntry, TypedPropertyQueryResult, u64_portable, u64_typed};

const INDEXED_SCHEME_ID: SchemeId = SchemeId(0x4301);
const ORDERBOOK_SCHEME_ID: SchemeId = SchemeId(0x4302);

fn profile(
    scheme_profile_id: SchemeProfileId,
    scheme_id: SchemeId,
    layout_kind: ColumnLayoutKind,
    supported_property_query_kinds: Vec<PropertyQueryKind>,
) -> SchemeProfile {
    SchemeProfile::new(
        scheme_profile_id,
        format!("scheme_{}_v1", scheme_id.raw()),
        None,
        scheme_id,
        match layout_kind {
            ColumnLayoutKind::SSMC_V1 => CommitmentContractKind::SortedStateMerkleChain,
            ColumnLayoutKind::SMT_V1 => CommitmentContractKind::SparseMerkleTree,
            other => CommitmentContractKind::Opaque {
                family: format!("layout_{:04x}", other.0),
            },
        },
        VerifierDigestFormat::FieldElementArray { width: 8 },
        supported_property_query_kinds,
        EncodingRequirements {
            field_family: FieldFamily::KoalaBear31,
            encoding_class: EncodingClass::FieldElementArray,
            width_constraint: match layout_kind {
                ColumnLayoutKind::SSMC_V1 => WidthConstraint::InclusiveRange { min: 1, max: 5 },
                _ => WidthConstraint::Any,
            },
            canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
            ordering_preserving: match layout_kind {
                ColumnLayoutKind::SSMC_V1 => Some(true),
                _ => None,
            },
        },
        layout_kind,
        RootProfileId::SMT_V1,
    )
    .expect("scheme profile")
}

fn registry_for_profile(profile: &SchemeProfile) -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    registry
        .register_scheme_profile(profile.clone())
        .expect("register scheme profile");
    registry
        .register_default_scheme_profile(
            profile.scheme_family_id,
            ENCODING_U64_ID,
            profile.scheme_profile_id,
        )
        .expect("register scheme mapping");
    registry.validate().expect("semantic registry");
    registry
}

fn indexed_profile() -> SchemeProfile {
    profile(
        SchemeProfileId(0x4301),
        INDEXED_SCHEME_ID,
        ColumnLayoutKind::SMT_V1,
        vec![],
    )
}

fn orderbook_profile() -> SchemeProfile {
    profile(
        SchemeProfileId(0x4302),
        ORDERBOOK_SCHEME_ID,
        ColumnLayoutKind::SSMC_V1,
        vec![PropertyQueryKind::Successor, PropertyQueryKind::Predecessor],
    )
}

fn source_artifact_for_profile(profile: &SchemeProfile) -> tabula_artifact::Artifact {
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
        profile.scheme_family_id.0
    );
    artifact_from_source_with_registry(&source, &registry_for_profile(profile))
}

fn orderbook_artifact() -> tabula_artifact::Artifact {
    source_artifact_for_profile(&orderbook_profile())
}

#[derive(Clone)]
struct SparseNativeBackend;

impl ColumnBackendFactory for SparseNativeBackend {
    fn scheme_id(&self) -> SchemeId {
        INDEXED_SCHEME_ID
    }

    fn name(&self) -> &str {
        "indexed_native"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != INDEXED_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "indexed backend expected id {} but received {}",
                INDEXED_SCHEME_ID.0, setup.profile.scheme_profile.scheme_family_id.0
            )));
        }
        SmtScheme::<3>.materialize_backend(setup)
    }
}

#[derive(Clone)]
struct OrderedNativeBackend;

impl ColumnBackendFactory for OrderedNativeBackend {
    fn scheme_id(&self) -> SchemeId {
        ORDERBOOK_SCHEME_ID
    }

    fn name(&self) -> &str {
        "orderbook_native"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != ORDERBOOK_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "orderbook backend expected id {} but received {}",
                ORDERBOOK_SCHEME_ID.0, setup.profile.scheme_profile.scheme_family_id.0
            )));
        }
        SsmcScheme::<3>.materialize_backend(setup)
    }
}

#[test]
fn native_platform_proves_and_verifies_custom_sparse_scheme() {
    let profile = indexed_profile();
    let artifact = source_artifact_for_profile(&profile);
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(TableId(0), ColId(0))
        .expect("resolve sparse column");
    assert_eq!(resolved.scheme_profile.scheme_family_id, INDEXED_SCHEME_ID);

    let state = single_cell_u64(TableId(0), ColId(0), RowKey(0), 10);
    let batch = single_tx_batch(0, vec![u64_portable(5)]);

    let host_environment = HostEnvironment::empty()
        .with_runtime_registries(RuntimeRegistries::standard())
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(SparseNativeBackend))
        .expect("register sparse backend");

    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .with_host_environment(host_environment)
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("verification succeeds");
}

#[test]
fn native_platform_exposes_ordered_property_queries_for_custom_backend() {
    let artifact = orderbook_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let host_environment = HostEnvironment::empty()
        .with_runtime_registries(RuntimeRegistries::standard())
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(OrderedNativeBackend))
        .expect("register ordered backend");

    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment)
        .build()
        .expect("runtime");

    let column = runtime
        .proof_program()
        .runtime_columns()
        .get(&(TableId(0), ColId(0)))
        .expect("runtime column");
    let state = vec![
        TypedColumnEntry {
            row_key: RowKey(1),
            value: u64_typed(10),
            is_null: false,
        },
        TypedColumnEntry {
            row_key: RowKey(3),
            value: u64_typed(20),
            is_null: false,
        },
    ];
    let successor = column
        .resolve_property(&PropertyQuery::Successor { key: RowKey(1) }, &state)
        .expect("successor query");
    assert_eq!(
        successor,
        TypedPropertyQueryResult {
            value: u64_typed(20),
            key: Some(RowKey(3)),
            is_null: false,
        }
    );
    let predecessor = column
        .resolve_property(&PropertyQuery::Predecessor { key: RowKey(1) }, &state)
        .expect("predecessor query");
    assert_eq!(
        predecessor,
        TypedPropertyQueryResult {
            value: u64_typed(0),
            key: None,
            is_null: true,
        }
    );
}

#[test]
fn native_platform_proves_custom_ordered_backend() {
    let artifact = source_artifact_for_profile(&orderbook_profile());
    let compiled = compiled_program_from_artifact(&artifact);
    let state: State = single_cell_u64(TableId(0), ColId(0), RowKey(0), 3);
    let batch = single_tx_batch(0, vec![u64_portable(1)]);

    let host_environment = HostEnvironment::empty()
        .with_runtime_registries(RuntimeRegistries::standard())
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(OrderedNativeBackend))
        .expect("register ordered backend");

    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .with_host_environment(host_environment)
        .build()
        .expect("verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("verification succeeds");
}
