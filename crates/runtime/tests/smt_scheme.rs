//! Integration tests for SMT-backed runtime and verifier scheme seams.
#![cfg(feature = "prove")]

use tabula_artifact::{
    ProgramArtifact, SchemeDescriptor, StateEntry, StateSnapshot, TransactionBatch,
    TransactionInput,
};
use tabula_compiler::{
    SchemeDescriptorCatalog, compile_program_source, register_program_artifact,
    register_program_definition, register_program_definition_with_scheme_catalog,
    transfer_example_bundle,
};
use tabula_core::{ColumnLayoutKind, RootProfileId, SchemeId, Value};
use tabula_machine::SetupError;
use tabula_runtime::{
    ColumnPlan, ColumnSchemeFactory, ColumnViews, ProgramVerifier, ProveInput, RuntimeError,
    SmtScheme, TabulaRuntime,
};

const ALIAS_SMT_ID: SchemeId = SchemeId(0x4200);

fn alias_smt_descriptor() -> SchemeDescriptor {
    SchemeDescriptor {
        scheme_id: ALIAS_SMT_ID,
        scheme_version: 1,
        layout_kind: ColumnLayoutKind::SMT_V1,
        params_hash: [0x42; 32],
        root_profile_id: RootProfileId::SMT_V1,
        supported_property_query_kinds: vec![],
    }
}

fn alias_smt_artifact(mut artifact: ProgramArtifact) -> ProgramArtifact {
    artifact.column_proof_plan[0].scheme_id = ALIAS_SMT_ID;
    artifact.column_proof_plan[0].scheme_descriptor = alias_smt_descriptor();
    artifact
}

struct AliasSmtScheme<const W: usize> {
    descriptor: SchemeDescriptor,
}

impl<const W: usize> AliasSmtScheme<W> {
    fn new(descriptor: SchemeDescriptor) -> Self {
        Self { descriptor }
    }
}

impl<const W: usize> ColumnSchemeFactory for AliasSmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        self.descriptor.clone()
    }

    fn name(&self) -> &str {
        "alias_smt"
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if plan.scheme_id != self.descriptor.scheme_id {
            return Err(SetupError::SetupFailed(format!(
                "alias SMT factory expected scheme id {} but received {}",
                self.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        SmtScheme::<W>.build_column(plan)
    }
}

fn mixed_program_artifact() -> ProgramArtifact {
    let source = "\
table balances {
    liquid: u64,
    shielded: u64 @smt,
}

tx bump(amount: u64) {
    let liquid_now = balances[0].liquid
    let shielded_now = balances[0].shielded
    balances[0].liquid = liquid_now + amount
    balances[0].shielded = shielded_now + amount
}
";
    let definition = compile_program_source(source).expect("compile mixed source");
    register_program_definition(&definition)
        .expect("register mixed source")
        .into_program_artifact()
}

fn alias_smt_program_artifact() -> ProgramArtifact {
    let source = "\
table balances {
    amount: u64 @scheme(16896),
}

tx bump(amount: u64) {
    let current = balances[0].amount
    balances[0].amount = current + amount
}
";
    let definition = compile_program_source(source).expect("compile alias source");
    let mut scheme_catalog = SchemeDescriptorCatalog::new();
    scheme_catalog.insert(ALIAS_SMT_ID, alias_smt_descriptor());
    register_program_definition_with_scheme_catalog(&definition, &scheme_catalog)
        .expect("register alias source")
        .into_program_artifact()
}

fn mixed_state() -> StateSnapshot {
    StateSnapshot {
        cells: vec![
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(10)),
            },
            StateEntry {
                table: 0,
                row: 0,
                col: 1,
                value: Some(Value::U64(20)),
            },
        ],
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

#[test]
fn smt_only_runtime_and_verifier_accept_builtin_smt_column() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let mut artifact = bundle.program.clone();
    artifact.column_proof_plan[0].scheme_id = SchemeId::SMT;
    artifact.column_proof_plan[0].scheme_descriptor = SchemeDescriptor::builtin_smt();

    let compiled = register_program_artifact(&artifact).expect("compiled program");
    let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
    let executed = runtime
        .execute(&bundle.state, &bundle.batch)
        .expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &bundle.state,
            batch: &bundle.batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    runtime
        .verify(&proved.proof, &proved.statement)
        .expect("runtime verify succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("external verifier succeeds");
}

#[test]
fn alias_smt_scheme_flows_from_source_registration_catalog() {
    let artifact = alias_smt_program_artifact();
    let compiled = register_program_artifact(&artifact).expect("compiled program");
    assert_eq!(compiled.column_proof_plan()[0].scheme_id, ALIAS_SMT_ID);

    let state = StateSnapshot {
        cells: vec![StateEntry {
            table: 0,
            row: 0,
            col: 0,
            value: Some(Value::U64(10)),
        }],
    };
    let batch = single_tx_batch(5);

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme(AliasSmtScheme::<3>::new(alias_smt_descriptor()))
        .expect("register alias SMT")
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

    let verifier = ProgramVerifier::builder(artifact)
        .with_scheme(AliasSmtScheme::<3>::new(alias_smt_descriptor()))
        .expect("register alias SMT")
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("alias verifier succeeds");
}

#[test]
fn mixed_ssmc_and_smt_columns_prove_and_verify() {
    let artifact = mixed_program_artifact();
    assert_eq!(artifact.column_proof_plan[0].scheme_id, SchemeId::SSMC);
    assert_eq!(artifact.column_proof_plan[1].scheme_id, SchemeId::SMT);

    let state = mixed_state();
    let batch = single_tx_batch(5);
    let compiled = register_program_artifact(&artifact).expect("compiled program");
    let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("mixed verifier succeeds");
}

#[test]
fn alias_smt_scheme_proves_and_verifies_via_public_seam() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let artifact = alias_smt_artifact(bundle.program.clone());
    let compiled = register_program_artifact(&artifact).expect("compiled program");

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme(AliasSmtScheme::<3>::new(alias_smt_descriptor()))
        .expect("register alias SMT")
        .build()
        .expect("runtime");
    let executed = runtime
        .execute(&bundle.state, &bundle.batch)
        .expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &bundle.state,
            batch: &bundle.batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = ProgramVerifier::builder(artifact)
        .with_scheme(AliasSmtScheme::<3>::new(alias_smt_descriptor()))
        .expect("register alias SMT")
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("alias verifier succeeds");
}

#[test]
fn runtime_rejects_alias_descriptor_params_mismatch() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let artifact = alias_smt_artifact(bundle.program);
    let compiled = register_program_artifact(&artifact).expect("compiled program");

    let mut mismatched = alias_smt_descriptor();
    mismatched.params_hash = [0x99; 32];

    let err = TabulaRuntime::builder(compiled)
        .with_scheme(AliasSmtScheme::<3>::new(mismatched))
        .expect("register mismatched alias SMT")
        .build()
        .expect_err("descriptor mismatch must fail");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("scheme descriptor mismatch"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_rejects_alias_descriptor_supported_property_mismatch() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let artifact = alias_smt_artifact(bundle.program);
    let compiled = register_program_artifact(&artifact).expect("compiled program");

    let mut mismatched = alias_smt_descriptor();
    mismatched.supported_property_query_kinds = vec![tabula_ir::PropertyQueryKind::Successor];

    let err = TabulaRuntime::builder(compiled)
        .with_scheme(AliasSmtScheme::<3>::new(mismatched))
        .expect("register mismatched alias SMT")
        .build()
        .expect_err("descriptor mismatch must fail");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("scheme descriptor mismatch"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_rejects_alias_descriptor_layout_mismatch() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let artifact = alias_smt_artifact(bundle.program);
    let compiled = register_program_artifact(&artifact).expect("compiled program");

    let mut mismatched = alias_smt_descriptor();
    mismatched.layout_kind = ColumnLayoutKind::SSMC_V1;

    let err = TabulaRuntime::builder(compiled)
        .with_scheme(AliasSmtScheme::<3>::new(mismatched))
        .expect("register mismatched alias SMT")
        .build()
        .expect_err("descriptor mismatch must fail");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("scheme descriptor mismatch"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn verifier_rejects_alias_root_profile_mismatch() {
    let bundle = transfer_example_bundle().expect("example bundle");
    let mut artifact = alias_smt_artifact(bundle.program);
    artifact.column_proof_plan[0]
        .scheme_descriptor
        .root_profile_id = RootProfileId(7);

    let mut custom = alias_smt_descriptor();
    custom.root_profile_id = RootProfileId(7);

    let err = ProgramVerifier::builder(artifact)
        .with_scheme(AliasSmtScheme::<3>::new(custom))
        .expect("register alias SMT")
        .build()
        .expect_err("root profile mismatch must fail");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("root profile"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
