//! Integration tests for SMT-backed runtime and verifier scheme seams.
#![cfg(feature = "prove")]

use tabula_artifact::Artifact;
use tabula_compiler::TRANSFER_EXAMPLE_TAB_SOURCE;
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
use tabula_runtime::{HostEnvironment, ProveInput, SmtScheme, TabulaRuntime, Verifier};
use tabula_testing::exec::{
    artifact_from_source, artifact_from_source_with_registry, compiled_program_from_artifact,
};
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::examples::transfer_example_artifact_case;
use tabula_testing::fixtures::state::single_cell_u64;
use tabula_types::u64_portable;

const ALIAS_SMT_ID: SchemeId = SchemeId(0x4200);
const ALIAS_SMT_PROFILE_ID: SchemeProfileId = SchemeProfileId(0x4200);

fn alias_smt_profile(root_profile_id: RootProfileId) -> SchemeProfile {
    SchemeProfile::new(
        ALIAS_SMT_PROFILE_ID,
        "alias_smt_v1",
        None,
        ALIAS_SMT_ID,
        CommitmentContractKind::SparseMerkleTree,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        vec![],
        EncodingRequirements {
            field_family: FieldFamily::KoalaBear31,
            encoding_class: EncodingClass::FieldElementArray,
            width_constraint: WidthConstraint::Any,
            canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
            ordering_preserving: None,
        },
        ColumnLayoutKind::SMT_V1,
        root_profile_id,
    )
    .expect("alias smt profile")
}

fn alias_smt_registry(root_profile_id: RootProfileId) -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    registry
        .register_scheme_profile(alias_smt_profile(root_profile_id))
        .expect("register alias smt profile");
    registry
        .register_default_scheme_profile(ALIAS_SMT_ID, ENCODING_U64_ID, ALIAS_SMT_PROFILE_ID)
        .expect("register alias smt mapping");
    registry.validate().expect("semantic registry");
    registry
}

#[derive(Clone)]
struct AliasSmtBackend;

impl ColumnBackendFactory for AliasSmtBackend {
    fn scheme_id(&self) -> SchemeId {
        ALIAS_SMT_ID
    }

    fn name(&self) -> &str {
        "alias_smt"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != ALIAS_SMT_ID {
            return Err(ExtError::validation(format!(
                "alias smt backend expected id {} but received {}",
                ALIAS_SMT_ID.0, setup.profile.scheme_profile.scheme_family_id.0
            )));
        }
        if setup.profile.proof_layout_family() != ColumnLayoutKind::SMT_V1 {
            return Err(ExtError::validation(format!(
                "alias smt backend expected layout {} but received {}",
                ColumnLayoutKind::SMT_V1.0,
                setup.profile.proof_layout_family().0
            )));
        }
        SmtScheme::<3>.materialize_backend(setup)
    }
}

fn transfer_artifact_with_scheme_annotation(annotation: &str) -> Artifact {
    let source = TRANSFER_EXAMPLE_TAB_SOURCE
        .replace("balance: u64,", &format!("balance: u64 {annotation},"));
    artifact_from_source(&source)
}

fn alias_smt_artifact() -> Artifact {
    let source = "\
table balances {
    amount: u64 @scheme(16896),
}

tx bump(amount: u64) {
    let current = balances[0].amount
    balances[0].amount = current + amount
}
";
    artifact_from_source_with_registry(source, &alias_smt_registry(RootProfileId::SMT_V1))
}

#[test]
fn smt_only_runtime_and_verifier_accept_builtin_smt_column() {
    let case = transfer_example_artifact_case();
    let artifact = transfer_artifact_with_scheme_annotation("@smt");

    let compiled = compiled_program_from_artifact(&artifact);
    let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
    let executed = runtime
        .execute(&case.state, &case.batch)
        .expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &case.state,
            batch: &case.batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    runtime
        .verify(&proved.proof, &proved.statement)
        .expect("runtime verify succeeds");

    let verifier = Verifier::builder(artifact)
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("external verifier succeeds");
}

#[test]
fn alias_smt_scheme_flows_from_source_registration_catalog() {
    let artifact = alias_smt_artifact();
    let compiled = compiled_program_from_artifact(&artifact);
    let resolved = compiled
        .resolve_column_profile(tabula_core::TableId(0), tabula_core::ColId(0))
        .expect("resolve alias smt column");
    assert_eq!(resolved.scheme_profile.scheme_family_id, ALIAS_SMT_ID);

    let state = single_cell_u64(
        tabula_core::TableId(0),
        tabula_core::ColId(0),
        tabula_core::RowKey(0),
        10,
    );
    let batch = single_tx_batch(0, vec![u64_portable(5)]);

    let host_environment = HostEnvironment::standard()
        .with_column_backend_bundle(ColumnBackendFactoryBundle::new(AliasSmtBackend))
        .expect("register alias SMT backend bundle");

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
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("alias verifier succeeds");
}
