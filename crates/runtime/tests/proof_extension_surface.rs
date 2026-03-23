//! Proof-extension surface contract.
#![cfg(feature = "prove")]

use tabula_core::{ColumnLayoutKind, RootProfileId, SchemeId, SchemeProfileId};
use tabula_ext::{
    ColumnBackendFactory, ColumnBackendFactoryBundle, ColumnBackendSetup, ExtError,
    MaterializedColumnBackend,
};
use tabula_profile::{
    CanonicalNullEncoding, CommitmentContractKind, ENCODING_U64_ID, EncodingClass,
    EncodingRequirements, FieldFamily, SchemeProfile, SemanticRegistry, TranscriptSerialization,
    VerifierDigestFormat, WidthConstraint, builtin_semantic_registry,
};
use tabula_runtime::{
    HostEnvironment, ProveInput, RuntimeRegistries, SsmcScheme, TabulaRuntime, Verifier,
};
use tabula_testing::exec::{artifact_from_source_with_registry, compiled_program_from_artifact};
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::state::single_cell_u64;
use tabula_types::u64_portable;

const CUSTOM_ORDERED_SCHEME_ID: SchemeId = SchemeId(0x7201);
const CUSTOM_ORDERED_SCHEME_PROFILE_ID: SchemeProfileId = SchemeProfileId(0x7201);

fn custom_scheme_profile() -> SchemeProfile {
    SchemeProfile::new(
        CUSTOM_ORDERED_SCHEME_PROFILE_ID,
        "custom_ordered_v1",
        None,
        CUSTOM_ORDERED_SCHEME_ID,
        CommitmentContractKind::SortedStateMerkleChain,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        vec![],
        EncodingRequirements {
            field_family: FieldFamily::KoalaBear31,
            encoding_class: EncodingClass::FieldElementArray,
            width_constraint: WidthConstraint::InclusiveRange { min: 1, max: 5 },
            canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
            ordering_preserving: Some(true),
        },
        ColumnLayoutKind::SSMC_V1,
        RootProfileId::SMT_V1,
    )
    .expect("custom scheme profile")
}

fn custom_registry() -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    registry
        .register_scheme_profile(custom_scheme_profile())
        .expect("register custom scheme profile");
    registry
        .register_default_scheme_profile(
            CUSTOM_ORDERED_SCHEME_ID,
            ENCODING_U64_ID,
            CUSTOM_ORDERED_SCHEME_PROFILE_ID,
        )
        .expect("register custom scheme mapping");
    registry.validate().expect("semantic registry");
    registry
}

fn custom_artifact() -> tabula_artifact::Artifact {
    let source = "\
table balances {
    amount: u64 @scheme(29185)
}

tx bump(amount: u64) {
    let current = balances[0].amount
    balances[0].amount = current + amount
}
";
    artifact_from_source_with_registry(source, &custom_registry())
}

#[derive(Clone)]
struct CustomOrderedBackend;

impl ColumnBackendFactory for CustomOrderedBackend {
    fn scheme_id(&self) -> SchemeId {
        CUSTOM_ORDERED_SCHEME_ID
    }

    fn name(&self) -> &str {
        "custom_ordered"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != CUSTOM_ORDERED_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "custom ordered backend expected id {} but received {}",
                CUSTOM_ORDERED_SCHEME_ID.0, setup.profile.scheme_profile.scheme_family_id.0
            )));
        }
        if setup.profile.proof_layout_family() != ColumnLayoutKind::SSMC_V1 {
            return Err(ExtError::validation(format!(
                "custom ordered backend expected layout {} but received {}",
                ColumnLayoutKind::SSMC_V1.0,
                setup.profile.proof_layout_family().0
            )));
        }
        SsmcScheme::<3>.materialize_backend(setup)
    }
}

#[test]
fn proof_extension_surface_proves_and_verifies_custom_scheme() {
    let artifact = custom_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(tabula_core::TableId(0), tabula_core::ColId(0))
        .expect("resolve column profile");
    assert_eq!(
        resolved.scheme_profile.scheme_family_id,
        CUSTOM_ORDERED_SCHEME_ID
    );

    let host_environment = HostEnvironment::empty()
        .with_runtime_registries(RuntimeRegistries::standard())
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(CustomOrderedBackend))
        .expect("register custom backend bundle");

    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");

    let state = single_cell_u64(
        tabula_core::TableId(0),
        tabula_core::ColId(0),
        tabula_core::RowKey(0),
        7,
    );
    let batch = single_tx_batch(0, vec![u64_portable(8)]);
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
