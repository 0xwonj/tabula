#![allow(missing_docs)]

use std::collections::BTreeMap;
#[cfg(feature = "prove")]
use std::sync::Arc;

use tabula_compiler::{
    TRANSFER_EXAMPLE_TAB_SOURCE, compile_program_source, register_program_definition,
    transfer_example_bundle,
};
#[cfg(feature = "prove")]
use tabula_core::{ColId, RootProfileId, RowKey, SchemeId, TableId};
use tabula_core::{ExecutionConsistencyStatus, Value};
#[cfg(feature = "prove")]
use tabula_ext::backend::ProofColumn;
#[cfg(feature = "prove")]
use tabula_ext::backend::scheme::{ColumnProofPreparer, ProofSchemeFactory};
#[cfg(feature = "prove")]
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn};
#[cfg(feature = "prove")]
use tabula_sdk::Artifact;
#[cfg(feature = "prove")]
use tabula_sdk::ext::{
    ColumnLayoutKind, PrecompileBundle, PropertyQueryKind, SchemeBundle, SchemeDescriptor,
};
use tabula_sdk::{Sdk, SdkError};
#[cfg(feature = "prove")]
use tabula_testing::extensions::precompile::{
    ConstantOnePrecompileHandler, ConstantOnePrecompileProofFactory, SequencePrecompileHandler,
    SequencePrecompileProofFactory, constant_one_precompile_descriptor,
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

#[cfg(feature = "prove")]
const CUSTOM_ORDERED_SCHEME_ID: SchemeId = SchemeId(0x7201);

fn state_values(state: &tabula_sdk::State) -> BTreeMap<(u32, u64, u16), Value> {
    state
        .cells
        .iter()
        .filter_map(|entry| {
            entry
                .value
                .map(|value| ((entry.table, entry.row, entry.col), value))
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
    assert_eq!(values.get(&(0, 0, 0)), Some(&Value::U64(750)));
    assert_eq!(values.get(&(0, 1, 0)), Some(&Value::U64(600)));
    assert_eq!(values.get(&(0, 2, 0)), Some(&Value::U64(350)));
}

#[cfg(feature = "prove")]
#[test]
fn execute_capability_program_prepares_runtime_lazily() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor.clone()),
            )
            .and_then(|bundle| {
                bundle.with_handler(ConstantOnePrecompileHandler::new(descriptor.precompile_id))
            })
            .expect("precompile bundle"),
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
fn sdk_builder_rejects_duplicate_scheme_and_precompile_registrations() {
    let scheme_err = Sdk::builder()
        .with_scheme(
            SchemeBundle::new(CustomOrderedRuntimeScheme, CustomOrderedProofScheme)
                .expect("scheme bundle"),
        )
        .expect("first scheme registration")
        .with_scheme(
            SchemeBundle::new(CustomOrderedRuntimeScheme, CustomOrderedProofScheme)
                .expect("scheme bundle"),
        )
        .expect_err("duplicate scheme registration should fail");
    assert!(matches!(scheme_err, SdkError::InvalidSchemeBundle { .. }));

    let descriptor = constant_one_precompile_descriptor(tabula_sdk::ext::PrecompileId(0x0001));
    let precompile_err = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor.clone()),
            )
            .and_then(|bundle| {
                bundle.with_handler(ConstantOnePrecompileHandler::new(descriptor.precompile_id))
            })
            .expect("precompile bundle"),
        )
        .expect("first precompile registration")
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor.clone()),
            )
            .and_then(|bundle| {
                bundle.with_handler(ConstantOnePrecompileHandler::new(descriptor.precompile_id))
            })
            .expect("precompile bundle"),
        )
        .expect_err("duplicate precompile registration should fail");
    assert!(matches!(
        precompile_err,
        SdkError::InvalidPrecompileBundle { .. }
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
        .with_scheme(
            SchemeBundle::new(CustomOrderedRuntimeScheme, CustomOrderedProofScheme)
                .expect("scheme bundle"),
        )
        .expect("register scheme")
        .build();

    let program = sdk.compile(source).expect("compile custom source");
    assert_eq!(
        program.artifact().column_proof_plan[0].scheme_id,
        CUSTOM_ORDERED_SCHEME_ID
    );

    let state = single_cell_u64(TableId(0), ColId(0), RowKey(0), 7);
    let batch = single_tx_batch(0, vec![Value::U64(8)]);
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
fn custom_precompile_bundle_roundtrip() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor.clone()),
            )
            .and_then(|bundle| {
                bundle.with_handler(ConstantOnePrecompileHandler::new(descriptor.precompile_id))
            })
            .expect("precompile bundle"),
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
fn verification_only_precompile_bundle_supports_verifier_but_rejects_execution() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor),
            )
            .expect("verification bundle"),
        )
        .expect("register verification-only precompile")
        .build();

    let artifact = precompile_requirement_artifact();
    sdk.verifier(artifact.clone())
        .expect("sdk verifier")
        .warm()
        .expect("verification-only bundle should build verifier");
    let program = sdk.open(artifact).expect("open artifact");
    program
        .verifier()
        .expect("program verifier")
        .warm()
        .expect("program verifier warm");

    let err = program
        .execute(
            &tabula_testing::fixtures::state::empty_state(),
            &single_tx_batch(1, vec![]),
        )
        .expect_err("execution should reject verification-only precompile bundle");
    match err {
        SdkError::Runtime(tabula_runtime::RuntimeError::ValidationFailed { detail }) => {
            assert!(detail.contains("verification only"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[cfg(feature = "prove")]
#[test]
fn multi_output_precompile_roundtrip_proves_large_transcript_payload() {
    let descriptor = sequence_precompile_descriptor_fixture();
    let sdk = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                SequencePrecompileProofFactory::new(descriptor.clone()),
            )
            .and_then(|bundle| {
                bundle.with_handler(SequencePrecompileHandler::new(descriptor.precompile_id))
            })
            .expect("sequence precompile bundle"),
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
fn custom_descriptor() -> SchemeDescriptor {
    SchemeDescriptor {
        scheme_id: CUSTOM_ORDERED_SCHEME_ID,
        scheme_version: 1,
        layout_kind: ColumnLayoutKind::SSMC_V1,
        params_hash: [0x72; 32],
        root_profile_id: RootProfileId::SMT_V1,
        supported_property_query_kinds: vec![PropertyQueryKind::Successor],
    }
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct CustomOrderedRuntimeScheme;

#[cfg(feature = "prove")]
impl ColumnSchemeFactory for CustomOrderedRuntimeScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor()
    }

    fn name(&self) -> &str {
        "custom_ordered"
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        tabula_runtime::SsmcScheme::<3>.build_runtime_column(plan)
    }
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct CustomOrderedProofScheme;

#[cfg(feature = "prove")]
impl ProofSchemeFactory for CustomOrderedProofScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor()
    }

    fn name(&self) -> &str {
        "custom_ordered"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        tabula_runtime::SsmcScheme::<3>.build_proof_column(plan)
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        tabula_runtime::SsmcScheme::<3>.build_proof_preparer(plan)
    }
}
