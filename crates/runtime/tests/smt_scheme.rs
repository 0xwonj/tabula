//! Integration tests for SMT-backed runtime and verifier scheme seams.
#![cfg(feature = "prove")]

use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use tabula_artifact::{Artifact, SchemeDescriptor, State};
use tabula_compiler::SchemeDescriptorCatalog;
use tabula_core::{ColumnLayoutKind, RootProfileId, SchemeId, Value};
use tabula_ext::backend::ProofColumn;
use tabula_ext::backend::scheme::{ColumnProofPreparer, ProofSchemeFactory};
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn, SchemeBundle};
use tabula_runtime::{ProveInput, RuntimeError, SmtScheme, TabulaRuntime, Verifier};
use tabula_testing::exec::{
    artifact_from_source, artifact_from_source_with_catalog, compiled_program_from_artifact,
};
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::examples::transfer_example_artifact_case;
use tabula_testing::fixtures::state::{liquid_shielded_state, single_cell_u64};

const ALIAS_SMT_ID: SchemeId = SchemeId(0x4200);
const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";

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

fn retag_with_alias_smt(mut artifact: Artifact) -> Artifact {
    artifact.column_proof_plan[0].scheme_id = ALIAS_SMT_ID;
    artifact.column_proof_plan[0].scheme_descriptor = alias_smt_descriptor();
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

struct AliasSmtScheme<const W: usize> {
    descriptor: SchemeDescriptor,
}

impl<const W: usize> AliasSmtScheme<W> {
    fn new(descriptor: SchemeDescriptor) -> Self {
        Self { descriptor }
    }
}

fn alias_smt_bundle<const W: usize>(descriptor: SchemeDescriptor) -> SchemeBundle {
    let runtime = AliasSmtScheme::<W>::new(descriptor.clone());
    let proof = AliasSmtScheme::<W>::new(descriptor);
    SchemeBundle::new(runtime, proof).expect("alias SMT bundle")
}

impl<const W: usize> ColumnSchemeFactory for AliasSmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        self.descriptor.clone()
    }

    fn name(&self) -> &str {
        "alias_smt"
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        if plan.scheme_id != self.descriptor.scheme_id {
            return Err(ExtError::validation(format!(
                "alias SMT factory expected scheme id {} but received {}",
                self.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        SmtScheme::<W>.build_runtime_column(plan)
    }
}

impl<const W: usize> ProofSchemeFactory for AliasSmtScheme<W> {
    fn descriptor(&self) -> SchemeDescriptor {
        self.descriptor.clone()
    }

    fn name(&self) -> &str {
        "alias_smt"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        if plan.scheme_id != self.descriptor.scheme_id {
            return Err(ExtError::validation(format!(
                "alias SMT proof factory expected scheme id {} but received {}",
                self.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        SmtScheme::<W>.build_proof_column(plan)
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        if plan.scheme_id != self.descriptor.scheme_id {
            return Err(ExtError::validation(format!(
                "alias SMT proof factory expected scheme id {} but received {}",
                self.descriptor.scheme_id.0, plan.scheme_id.0,
            )));
        }
        SmtScheme::<W>.build_proof_preparer(plan)
    }
}

fn mixed_artifact() -> Artifact {
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
    artifact_from_source(source)
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
    let mut scheme_catalog = SchemeDescriptorCatalog::new();
    scheme_catalog.insert(ALIAS_SMT_ID, alias_smt_descriptor());
    artifact_from_source_with_catalog(source, &scheme_catalog)
}

fn mixed_state() -> State {
    liquid_shielded_state(10, 20)
}

#[test]
fn smt_only_runtime_and_verifier_accept_builtin_smt_column() {
    let case = transfer_example_artifact_case();
    let mut artifact = case.artifact.clone();
    artifact.column_proof_plan[0].scheme_id = SchemeId::SMT;
    artifact.column_proof_plan[0].scheme_descriptor = SchemeDescriptor::builtin_smt();
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.column_proof_plan,
        )
        .expect("semantic hash"),
    );

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
    assert_eq!(compiled.column_proof_plan()[0].scheme_id, ALIAS_SMT_ID);

    let state = single_cell_u64(
        tabula_core::TableId(0),
        tabula_core::ColId(0),
        tabula_core::RowKey(0),
        10,
    );
    let batch = single_tx_batch(0, vec![Value::U64(5)]);

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(alias_smt_bundle::<3>(alias_smt_descriptor()))
        .expect("register alias SMT bundle")
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
        .with_scheme_bundle(alias_smt_bundle::<3>(alias_smt_descriptor()))
        .expect("register alias SMT bundle")
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("alias verifier succeeds");
}

#[test]
fn mixed_ssmc_and_smt_columns_prove_and_verify() {
    let artifact = mixed_artifact();
    assert_eq!(artifact.column_proof_plan[0].scheme_id, SchemeId::SSMC);
    assert_eq!(artifact.column_proof_plan[1].scheme_id, SchemeId::SMT);

    let state = mixed_state();
    let batch = single_tx_batch(0, vec![Value::U64(5)]);
    let compiled = compiled_program_from_artifact(&artifact);
    let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
    let executed = runtime.execute(&state, &batch).expect("execution succeeds");
    let proved = runtime
        .prove(&ProveInput {
            state: &state,
            batch: &batch,
            executed: &executed,
        })
        .expect("proof succeeds");

    let verifier = Verifier::builder(artifact)
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("mixed verifier succeeds");
}

#[test]
fn alias_smt_scheme_proves_and_verifies_via_public_seam() {
    let case = transfer_example_artifact_case();
    let artifact = retag_with_alias_smt(case.artifact.clone());
    let compiled = compiled_program_from_artifact(&artifact);

    let runtime = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(alias_smt_bundle::<3>(alias_smt_descriptor()))
        .expect("register alias SMT bundle")
        .build()
        .expect("runtime");
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

    let verifier = Verifier::builder(artifact)
        .with_scheme_bundle(alias_smt_bundle::<3>(alias_smt_descriptor()))
        .expect("register alias SMT bundle")
        .build()
        .expect("program verifier");
    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("alias verifier succeeds");
}

#[test]
fn runtime_rejects_alias_descriptor_params_mismatch() {
    let case = transfer_example_artifact_case();
    let artifact = retag_with_alias_smt(case.artifact);
    let compiled = compiled_program_from_artifact(&artifact);

    let mut mismatched = alias_smt_descriptor();
    mismatched.params_hash = [0x99; 32];

    let err = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(alias_smt_bundle::<3>(mismatched))
        .expect("register mismatched alias SMT bundle")
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
    let case = transfer_example_artifact_case();
    let artifact = retag_with_alias_smt(case.artifact);
    let compiled = compiled_program_from_artifact(&artifact);

    let mut mismatched = alias_smt_descriptor();
    mismatched.supported_property_query_kinds = vec![tabula_ir::PropertyQueryKind::Successor];

    let err = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(alias_smt_bundle::<3>(mismatched))
        .expect("register mismatched alias SMT bundle")
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
    let case = transfer_example_artifact_case();
    let artifact = retag_with_alias_smt(case.artifact);
    let compiled = compiled_program_from_artifact(&artifact);

    let mut mismatched = alias_smt_descriptor();
    mismatched.layout_kind = ColumnLayoutKind::SSMC_V1;

    let err = TabulaRuntime::builder(compiled)
        .with_scheme_bundle(alias_smt_bundle::<3>(mismatched))
        .expect("register mismatched alias SMT bundle")
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
    let case = transfer_example_artifact_case();
    let mut artifact = retag_with_alias_smt(case.artifact);
    artifact.column_proof_plan[0]
        .scheme_descriptor
        .root_profile_id = RootProfileId(7);
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.column_proof_plan,
        )
        .expect("semantic hash"),
    );

    let mut custom = alias_smt_descriptor();
    custom.root_profile_id = RootProfileId(7);

    let err = Verifier::builder(artifact)
        .with_scheme_bundle(alias_smt_bundle::<3>(custom))
        .expect("register alias SMT bundle")
        .build()
        .expect_err("root profile mismatch must fail");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("root profile"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
