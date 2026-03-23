//! Canonical custom backend registration surface contract.
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
use tabula_runtime::{HostEnvironment, HostTypeRuntimes, SsmcScheme, TabulaRuntime, Verifier};
use tabula_testing::exec::{artifact_from_source_with_registry, compiled_program_from_artifact};

const STABLE_ONLY_SCHEME_ID: SchemeId = SchemeId(0x7101);
const STABLE_ONLY_SCHEME_PROFILE_ID: SchemeProfileId = SchemeProfileId(0x7101);

fn stable_scheme_profile() -> SchemeProfile {
    SchemeProfile::new(
        STABLE_ONLY_SCHEME_PROFILE_ID,
        "stable_only_v1",
        None,
        STABLE_ONLY_SCHEME_ID,
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
    .expect("stable scheme profile")
}

fn stable_registry() -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    registry
        .register_scheme_profile(stable_scheme_profile())
        .expect("register stable scheme profile");
    registry
        .register_default_scheme_profile(
            STABLE_ONLY_SCHEME_ID,
            ENCODING_U64_ID,
            STABLE_ONLY_SCHEME_PROFILE_ID,
        )
        .expect("register stable scheme mapping");
    registry.validate().expect("semantic registry");
    registry
}

fn stable_artifact() -> tabula_artifact::Artifact {
    let source = "\
table balances {
    amount: u64 @scheme(28929)
}

tx noop() {}
";
    artifact_from_source_with_registry(source, &stable_registry())
}

#[derive(Clone)]
struct StableOnlyBackend;

impl ColumnBackendFactory for StableOnlyBackend {
    fn scheme_id(&self) -> SchemeId {
        STABLE_ONLY_SCHEME_ID
    }

    fn name(&self) -> &str {
        "stable_only"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != STABLE_ONLY_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "stable backend expected id {} but received {}",
                STABLE_ONLY_SCHEME_ID.0, setup.profile.scheme_profile.scheme_family_id.0
            )));
        }
        SsmcScheme::<3>.materialize_backend(setup)
    }
}

#[test]
fn stable_scheme_surface_uses_only_canonical_backend_registration() {
    let artifact = stable_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(tabula_core::TableId(0), tabula_core::ColId(0))
        .expect("resolve stable column");
    assert_eq!(
        resolved.scheme_profile.scheme_family_id,
        STABLE_ONLY_SCHEME_ID
    );

    let host_environment = HostEnvironment::empty()
        .with_type_runtimes(HostTypeRuntimes::standard())
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(StableOnlyBackend))
        .expect("register stable backend");

    let runtime = TabulaRuntime::builder(compiled)
        .with_host_environment(host_environment.clone())
        .build()
        .expect("runtime");
    assert_eq!(runtime.resolved_program().runtime_columns().len(), 1);

    let verifier = Verifier::builder(artifact)
        .with_host_environment(host_environment)
        .build()
        .expect("verifier");
    assert_eq!(verifier.binding().metadata_hash().len(), 64);
}
