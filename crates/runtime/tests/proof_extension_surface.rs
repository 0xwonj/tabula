//! Proof-extension surface contract.
#![cfg(feature = "prove")]

use std::sync::Arc;

use tabula_artifact::SchemeDescriptor;
use tabula_compiler::SchemeDescriptorCatalog;
use tabula_core::{ColumnLayoutKind, RootProfileId, SchemeId, Value};
use tabula_ext::backend::ProofColumn;
use tabula_ext::backend::scheme::{ColumnProofPreparer, ProofSchemeFactory};
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn, SchemeBundle};
use tabula_runtime::{ProveInput, SsmcScheme, TabulaRuntime, Verifier};
use tabula_testing::exec::{artifact_from_source_with_catalog, compiled_program_from_artifact};
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::state::single_cell_u64;

const CUSTOM_ORDERED_SCHEME_ID: SchemeId = SchemeId(0x7201);

fn custom_descriptor() -> SchemeDescriptor {
    SchemeDescriptor {
        scheme_id: CUSTOM_ORDERED_SCHEME_ID,
        scheme_version: 1,
        layout_kind: ColumnLayoutKind::SSMC_V1,
        params_hash: [0x72; 32],
        root_profile_id: RootProfileId::SMT_V1,
        supported_property_query_kinds: vec![],
    }
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
    let mut catalog = SchemeDescriptorCatalog::new();
    catalog.insert(CUSTOM_ORDERED_SCHEME_ID, custom_descriptor());
    artifact_from_source_with_catalog(source, &catalog)
}

#[derive(Clone)]
struct CustomOrderedRuntimeScheme;

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
        if plan.scheme_id != CUSTOM_ORDERED_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "custom ordered runtime scheme expected id {} but received {}",
                CUSTOM_ORDERED_SCHEME_ID.0, plan.scheme_id.0
            )));
        }
        SsmcScheme::<3>.build_runtime_column(plan)
    }
}

#[derive(Clone)]
struct CustomOrderedProofScheme;

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
        if plan.scheme_id != CUSTOM_ORDERED_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "custom ordered proof scheme expected id {} but received {}",
                CUSTOM_ORDERED_SCHEME_ID.0, plan.scheme_id.0
            )));
        }
        SsmcScheme::<3>.build_proof_column(plan)
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        if plan.scheme_id != CUSTOM_ORDERED_SCHEME_ID {
            return Err(ExtError::validation(format!(
                "custom ordered proof scheme expected id {} but received {}",
                CUSTOM_ORDERED_SCHEME_ID.0, plan.scheme_id.0
            )));
        }
        SsmcScheme::<3>.build_proof_preparer(plan)
    }
}

#[test]
fn proof_extension_surface_proves_and_verifies_custom_scheme() {
    let artifact = custom_artifact();
    let compiled = compiled_program_from_artifact(&artifact);

    let runtime = TabulaRuntime::builder(compiled)
        .without_default_schemes()
        .with_scheme_bundle(
            SchemeBundle::new(CustomOrderedRuntimeScheme, CustomOrderedProofScheme)
                .expect("custom scheme bundle"),
        )
        .expect("register custom scheme bundle")
        .build()
        .expect("runtime");

    let state = single_cell_u64(
        tabula_core::TableId(0),
        tabula_core::ColId(0),
        tabula_core::RowKey(0),
        7,
    );
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
        .without_default_schemes()
        .with_scheme_bundle(
            SchemeBundle::new(CustomOrderedRuntimeScheme, CustomOrderedProofScheme)
                .expect("custom ordered bundle"),
        )
        .expect("register custom proof bundle")
        .build()
        .expect("verifier");

    verifier
        .verify(&proved.proof, &proved.statement)
        .expect("verification succeeds");
}
