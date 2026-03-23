#![allow(missing_docs)]

use std::collections::BTreeMap;

#[cfg(feature = "prove")]
use tabula_compiler::ProgramDefinition;
use tabula_compiler::{
    TRANSFER_EXAMPLE_TAB_SOURCE, compile_program_source, register_program_definition,
    transfer_example_bundle,
};
use tabula_core::ExecutionConsistencyStatus;
#[cfg(feature = "prove")]
use tabula_core::{ColId, RootProfileId, RowKey, SchemeId, TableId, TxTypeId};
#[cfg(feature = "prove")]
use tabula_ext::{ColumnBackendFactory, ColumnBackendSetup, ExtError, MaterializedColumnBackend};
#[cfg(feature = "prove")]
use tabula_ir::{Instruction, TxTypeDef};
#[cfg(feature = "prove")]
use tabula_profile::{
    CanonicalNullEncoding, CommitmentContractKind, ENCODING_U64_ID, EncodingClass,
    EncodingRequirements, FieldFamily, SchemeProfile, SemanticRegistry, TranscriptSerialization,
    VerifierDigestFormat, WidthConstraint, builtin_semantic_registry,
};
#[cfg(feature = "prove")]
use tabula_sdk::Artifact;
#[cfg(feature = "prove")]
use tabula_sdk::ext::{
    ColumnBackendFactoryBundle, ColumnLayoutKind, PrecompileBackendFactoryBundle, PrecompileId,
    PropertyQueryKind,
};
use tabula_sdk::{Sdk, SdkError};
#[cfg(feature = "prove")]
use tabula_testing::extensions::precompile::{
    ConstantOnePrecompileBackendFactory, SequencePrecompileBackendFactory,
    constant_one_precompile_descriptor,
};
#[cfg(feature = "prove")]
use tabula_testing::fixtures::artifacts::{
    precompile_requirement_artifact, precompile_requirement_descriptor,
    sequence_precompile_artifact, sequence_precompile_descriptor_fixture,
};
#[cfg(feature = "prove")]
use tabula_testing::fixtures::batch::single_tx_batch;
#[cfg(feature = "prove")]
use tabula_testing::fixtures::compiled::compiled_single_write_program;
#[cfg(feature = "prove")]
use tabula_testing::fixtures::state::single_cell_u64;
use tabula_types::u64_portable;

#[cfg(feature = "prove")]
const CUSTOM_ORDERED_SCHEME_ID: SchemeId = SchemeId(0x7201);

fn state_values(
    state: &tabula_sdk::State,
) -> BTreeMap<(u32, u64, u16), tabula_core::PortableValue> {
    state
        .cells
        .iter()
        .filter_map(|entry| {
            entry
                .value
                .as_ref()
                .map(|value| ((entry.table, entry.row, entry.col), value.clone()))
        })
        .collect()
}

#[test]
fn compile_source_to_program() {
    let sdk = Sdk::standard();
    let program = sdk
        .compile(TRANSFER_EXAMPLE_TAB_SOURCE)
        .expect("compile source");
    let bundle = transfer_example_bundle().expect("transfer example bundle");

    assert_eq!(
        program.artifact().canonical_digest().expect("sdk digest"),
        bundle.program.canonical_digest().expect("bundle digest")
    );
}

#[test]
fn register_definition_to_program() {
    let sdk = Sdk::standard();
    let definition = compile_program_source(TRANSFER_EXAMPLE_TAB_SOURCE).expect("compile");
    let program = sdk.register(&definition).expect("register through sdk");
    let compiled = register_program_definition(&definition).expect("register through compiler");

    assert_eq!(
        program.artifact().canonical_digest().expect("sdk digest"),
        compiled
            .as_artifact()
            .canonical_digest()
            .expect("compiler digest")
    );
}

#[test]
fn open_artifact_to_program() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let program = sdk.open(bundle.program.clone()).expect("open artifact");

    assert_eq!(
        program
            .artifact()
            .canonical_digest()
            .expect("program digest"),
        bundle.program.canonical_digest().expect("bundle digest")
    );
}

#[test]
fn open_invalid_artifact_fails_closed() {
    let sdk = Sdk::standard();
    let mut bundle = transfer_example_bundle().expect("transfer example bundle");
    bundle.program.contract_metadata.profile_hash = [0x99; 32];

    let err = sdk
        .open(bundle.program)
        .expect_err("invalid artifact should fail");
    assert!(matches!(err, SdkError::Compiler(_)));
}

#[test]
fn execute_simple_program() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let program = sdk
        .compile(TRANSFER_EXAMPLE_TAB_SOURCE)
        .expect("compile source");
    let execution = program
        .execute(&bundle.state, &bundle.batch)
        .expect("execute transfer example");

    assert_eq!(execution.consistency(), ExecutionConsistencyStatus::Passed);
    assert_eq!(execution.txs().len(), 3);
    assert!(!execution.read_set().is_empty());
    assert_eq!(execution.write_set().len(), 3);

    let values = state_values(execution.state_after());
    assert_eq!(values.get(&(0, 0, 0)), Some(&u64_portable(750)));
    assert_eq!(values.get(&(0, 1, 0)), Some(&u64_portable(600)));
    assert_eq!(values.get(&(0, 2, 0)), Some(&u64_portable(350)));
}

#[cfg(feature = "prove")]
#[test]
fn execute_capability_program_prepares_runtime_lazily() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile_support(
            descriptor.clone(),
            constant_one_backend_bundle(descriptor.clone()),
        )
        .expect("register precompile")
        .build();

    let artifact = precompile_requirement_artifact();
    let program = sdk.open(artifact).expect("open artifact");
    let execution = program
        .execute(
            &tabula_testing::fixtures::state::empty_state(),
            &single_tx_batch(1, vec![]),
        )
        .expect("execute through lazy runtime");

    assert_eq!(execution.txs().len(), 1);
}

#[cfg(feature = "prove")]
#[test]
fn prove_and_verify() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let program = sdk.open(bundle.program.clone()).expect("open artifact");
    let execution = program
        .execute(&bundle.state, &bundle.batch)
        .expect("execute");
    let proof = program.prove(&execution).expect("prove");

    assert_eq!(
        proof.statement().program_hash,
        bundle.program.canonical_digest().expect("artifact digest")
    );
    assert!(proof.summary().chip_count > 0);

    program
        .verifier()
        .expect("program verifier")
        .verify(&proof)
        .expect("program verifier accepts proof");

    sdk.verifier(bundle.program.clone())
        .expect("sdk verifier")
        .verify(&proof)
        .expect("sdk verifier accepts proof");

    let wrong_artifact: Artifact = compiled_single_write_program().into_artifact();
    let err = sdk
        .verifier(wrong_artifact)
        .expect("wrong verifier")
        .verify(&proof)
        .expect_err("wrong verifier should reject proof");
    assert!(matches!(err, SdkError::Runtime(_)));
}

#[cfg(feature = "prove")]
#[test]
fn program_and_verifier_warm_and_reuse() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let program = sdk.open(bundle.program.clone()).expect("open artifact");

    program.warm().expect("warm program");
    let verifier = program.verifier().expect("program verifier");
    verifier.warm().expect("warm verifier");

    let first = program
        .execute_and_prove(&bundle.state, &bundle.batch)
        .expect("first proof");
    verifier.verify(&first).expect("verify first proof");

    let second = program
        .execute_and_prove(&bundle.state, &bundle.batch)
        .expect("second proof");
    verifier.verify(&second).expect("verify second proof");

    assert_eq!(
        first.statement().statement_hash(),
        second.statement().statement_hash()
    );
}

#[cfg(feature = "prove")]
#[test]
fn prove_rejects_execution_from_different_program() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let transfer_program = sdk.open(bundle.program).expect("open transfer artifact");
    let single_write_program = sdk
        .open(compiled_single_write_program().into_artifact())
        .expect("open single write artifact");
    let execution = single_write_program
        .execute(
            &single_cell_u64(TableId(1), ColId(0), RowKey(0), 1),
            &single_tx_batch(1, vec![]),
        )
        .expect("execute single write program");

    let err = transfer_program
        .prove(&execution)
        .expect_err("mismatched execution should fail");
    assert!(matches!(err, SdkError::ExecutionProgramMismatch));
}

#[cfg(feature = "prove")]
#[test]
fn execute_and_prove_convenience() {
    let sdk = Sdk::standard();
    let bundle = transfer_example_bundle().expect("transfer example bundle");
    let program = sdk.open(bundle.program.clone()).expect("open artifact");

    let execution = program
        .execute(&bundle.state, &bundle.batch)
        .expect("execute");
    let proof = program.prove(&execution).expect("prove");
    let convenience = program
        .execute_and_prove(&bundle.state, &bundle.batch)
        .expect("execute and prove");

    assert_eq!(
        proof.statement().statement_hash(),
        convenience.statement().statement_hash()
    );
}

#[cfg(feature = "prove")]
#[test]
fn sdk_builder_rejects_duplicate_scheme_descriptor_and_backend_registrations() {
    let scheme_err = Sdk::builder()
        .with_column_backend(ColumnBackendFactoryBundle::new(CustomOrderedBackend))
        .expect("first scheme registration")
        .with_column_backend(ColumnBackendFactoryBundle::new(CustomOrderedBackend))
        .expect_err("duplicate scheme registration should fail");
    assert!(matches!(
        scheme_err,
        SdkError::InvalidColumnBackendBundle { .. }
    ));

    let descriptor = constant_one_precompile_descriptor(tabula_sdk::ext::PrecompileId(0x0001));
    let descriptor_err = Sdk::builder()
        .with_precompile_descriptor(descriptor.clone())
        .expect("first descriptor registration")
        .with_precompile_descriptor(descriptor.clone())
        .expect_err("duplicate descriptor registration should fail");
    assert!(matches!(
        descriptor_err,
        SdkError::InvalidPrecompileDescriptorRegistration { .. }
    ));

    let precompile_err = Sdk::builder()
        .with_precompile_backend(constant_one_backend_bundle(descriptor.clone()))
        .expect("first precompile backend registration")
        .with_precompile_backend(constant_one_backend_bundle(descriptor))
        .expect_err("duplicate precompile backend registration should fail");
    assert!(matches!(
        precompile_err,
        SdkError::InvalidPrecompileBackendBundle { .. }
    ));
}

#[cfg(feature = "prove")]
#[test]
fn custom_scheme_bundle_roundtrip() {
    let source = "\
table balances {
    amount: u64 @scheme(29185)
}

tx bump(amount: u64) {
    let current = balances[0].amount
    balances[0].amount = current + amount
}
";

    let sdk = Sdk::builder()
        .with_semantic_registry(custom_semantic_registry())
        .expect("register custom semantic registry")
        .with_column_backend(ColumnBackendFactoryBundle::new(CustomOrderedBackend))
        .expect("register backend")
        .build();

    let program = sdk.compile(source).expect("compile custom source");
    let resolved = program
        .artifact()
        .resolve_column_profile(TableId(0), ColId(0))
        .expect("resolve custom column");
    assert_eq!(
        resolved.scheme_profile.scheme_family_id,
        CUSTOM_ORDERED_SCHEME_ID
    );

    let state = single_cell_u64(TableId(0), ColId(0), RowKey(0), 7);
    let batch = single_tx_batch(0, vec![u64_portable(8)]);
    let execution = program.execute(&state, &batch).expect("execute");
    let proof = program.prove(&execution).expect("prove");

    program
        .verifier()
        .expect("verifier")
        .verify(&proof)
        .expect("verify");
}

#[cfg(feature = "prove")]
#[test]
fn custom_precompile_support_roundtrip() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile_support(
            descriptor.clone(),
            constant_one_backend_bundle(descriptor.clone()),
        )
        .expect("register precompile")
        .build();

    let state = tabula_testing::fixtures::state::empty_state();
    let batch = single_tx_batch(1, vec![]);
    let program = sdk
        .open(precompile_requirement_artifact())
        .expect("open precompile artifact");
    let execution = program.execute(&state, &batch).expect("execute");
    let proof = program.prove(&execution).expect("prove");

    assert_eq!(execution.txs().len(), 1);
    program
        .verifier()
        .expect("program verifier")
        .verify(&proof)
        .expect("program verifier verify");
    sdk.verifier(program.artifact().clone())
        .expect("sdk verifier")
        .verify(&proof)
        .expect("sdk verifier verify");
}

#[cfg(feature = "prove")]
#[test]
fn descriptor_only_precompile_registration_keeps_opening_but_rejects_host_builds() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile_descriptor(descriptor)
        .expect("register descriptor")
        .build();

    let artifact = precompile_requirement_artifact();
    let verifier_err = sdk
        .verifier(artifact.clone())
        .expect("sdk verifier")
        .warm()
        .expect_err("verifier should reject missing host backend");
    match verifier_err {
        SdkError::Runtime(tabula_runtime::RuntimeError::ValidationFailed { detail }) => {
            assert!(detail.contains("precompile backend"));
        }
        other => panic!("unexpected verifier error: {other}"),
    }

    let program = sdk.open(artifact).expect("open artifact");
    let warm_err = program
        .verifier()
        .expect("program verifier")
        .warm()
        .expect_err("program verifier should reject missing host backend");
    match warm_err {
        SdkError::Runtime(tabula_runtime::RuntimeError::ValidationFailed { detail }) => {
            assert!(detail.contains("precompile backend"));
        }
        other => panic!("unexpected program verifier error: {other}"),
    }

    let err = program
        .execute(
            &tabula_testing::fixtures::state::empty_state(),
            &single_tx_batch(1, vec![]),
        )
        .expect_err("execution should reject descriptor-only precompile support");
    match err {
        SdkError::Runtime(tabula_runtime::RuntimeError::ValidationFailed { detail }) => {
            assert!(detail.contains("precompile backend"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[cfg(feature = "prove")]
#[test]
fn backend_only_precompile_support_does_not_enable_source_registration() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile_backend(constant_one_backend_bundle(descriptor.clone()))
        .expect("register host backend")
        .build();

    let definition = precompile_requirement_definition(descriptor.precompile_id);
    let err = sdk
        .register(&definition)
        .expect_err("source registration should reject missing compiler descriptor");
    assert!(matches!(err, SdkError::Compiler(_)));
}

#[cfg(feature = "prove")]
#[test]
fn multi_output_precompile_roundtrip_proves_large_transcript_payload() {
    let descriptor = sequence_precompile_descriptor_fixture();
    let sdk = Sdk::builder()
        .with_precompile_support(
            descriptor.clone(),
            sequence_backend_bundle(descriptor.clone()),
        )
        .expect("register sequence precompile")
        .build();

    let program = sdk
        .open(sequence_precompile_artifact())
        .expect("open sequence precompile artifact");
    let execution = program
        .execute(
            &tabula_testing::fixtures::state::empty_state(),
            &single_tx_batch(1, vec![]),
        )
        .expect("execute");
    let proof = program.prove(&execution).expect("prove");

    assert_eq!(execution.txs().len(), 1);
    program
        .verifier()
        .expect("verifier")
        .verify(&proof)
        .expect("verify");
}

#[cfg(feature = "prove")]
fn custom_scheme_profile() -> SchemeProfile {
    SchemeProfile::new(
        tabula_core::SchemeProfileId(0x7201),
        "custom_ordered_v1",
        None,
        CUSTOM_ORDERED_SCHEME_ID,
        CommitmentContractKind::SortedStateMerkleChain,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        vec![PropertyQueryKind::Successor],
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
    .expect("custom ordered scheme profile")
}

#[cfg(feature = "prove")]
fn custom_semantic_registry() -> SemanticRegistry {
    let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
    registry
        .register_scheme_profile(custom_scheme_profile())
        .expect("register custom scheme profile");
    registry
        .register_default_scheme_profile(
            CUSTOM_ORDERED_SCHEME_ID,
            ENCODING_U64_ID,
            tabula_core::SchemeProfileId(0x7201),
        )
        .expect("register custom scheme mapping");
    registry.validate().expect("semantic registry");
    registry
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct CustomOrderedBackend;

#[cfg(feature = "prove")]
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
        tabula_runtime::SsmcScheme::<3>.materialize_backend(setup)
    }
}

#[cfg(feature = "prove")]
fn constant_one_backend_bundle(
    descriptor: tabula_sdk::ext::PrecompileDescriptor,
) -> PrecompileBackendFactoryBundle {
    PrecompileBackendFactoryBundle::new(ConstantOnePrecompileBackendFactory::new(descriptor))
}

#[cfg(feature = "prove")]
fn sequence_backend_bundle(
    descriptor: tabula_sdk::ext::PrecompileDescriptor,
) -> PrecompileBackendFactoryBundle {
    PrecompileBackendFactoryBundle::new(SequencePrecompileBackendFactory::new(descriptor))
}

#[cfg(feature = "prove")]
fn precompile_requirement_definition(precompile_id: PrecompileId) -> ProgramDefinition {
    ProgramDefinition {
        table_schemas: vec![],
        tx_types: vec![TxTypeDef {
            id: TxTypeId(1),
            name: "scan".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Precompile {
                id: precompile_id,
                dst_slots: vec![0],
                inputs: vec![],
            }],
        }],
        column_schemes: vec![],
    }
}
