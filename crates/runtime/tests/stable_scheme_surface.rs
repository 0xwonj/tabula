//! Stable runtime surface contract: custom schemes register through bundles.
#![cfg(feature = "prove")]

use std::sync::Arc;

use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tabula_artifact::SchemeDescriptor;
use tabula_compiler::register_program;
use tabula_core::error::TabulaError;
use tabula_core::{
    ColId, ColumnDef, ColumnLayoutKind, RootProfileId, SchemeId, TableId, TableSchema, TxTypeId,
    ValueType,
};
use tabula_ext::backend::scheme::{
    ColumnProofContext, ColumnProofPreparer, PreparedColumnProof, ProofSchemeFactory,
};
use tabula_ext::backend::{ChipIdAllocator, ColumnChipSet, ProofColumn, SetupError};
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn, SchemeBundle};
use tabula_ir::TxTypeDef;
use tabula_runtime::TabulaRuntime;
use tabula_testing::exec::compiled_program_from_artifact;

const STABLE_ONLY_SCHEME_ID: SchemeId = SchemeId(0x7101);
const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";

#[derive(Clone)]
struct StableOnlyRuntimeScheme;

struct StableOnlyRuntimeColumn;

impl RuntimeColumn for StableOnlyRuntimeColumn {
    fn name(&self) -> &str {
        "stable_only"
    }
}

impl ColumnSchemeFactory for StableOnlyRuntimeScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        SchemeDescriptor {
            scheme_id: STABLE_ONLY_SCHEME_ID,
            scheme_version: 1,
            layout_kind: ColumnLayoutKind::SSMC_V1,
            params_hash: [0x71; 32],
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds: vec![],
        }
    }

    fn name(&self) -> &str {
        "stable_only"
    }

    fn build_runtime_column(
        &self,
        _plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        Ok(Arc::new(StableOnlyRuntimeColumn))
    }
}

struct StableOnlyProofColumn {
    plan: ResolvedColumnPlan,
}

impl ProofColumn for StableOnlyProofColumn {
    fn name(&self) -> &str {
        "stable_only"
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

    fn create_chips(&self, _alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        Ok(ColumnChipSet {
            airs: vec![],
            dyn_chips: vec![],
            bus_consumers: vec![],
        })
    }
}

struct StableOnlyProofPreparer {
    plan: ResolvedColumnPlan,
}

impl ColumnProofPreparer for StableOnlyProofPreparer {
    fn name(&self) -> &str {
        "stable_only"
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn prepare_column(
        &self,
        _context: ColumnProofContext,
    ) -> Result<PreparedColumnProof, ExtError> {
        Err(ExtError::proof_preparation(TabulaError::ProofError {
            phase: "stable_scheme_surface",
            detail: "stable-only proof preparer should not run in this validation test".to_string(),
        }))
    }
}

#[derive(Clone)]
struct StableOnlyProofScheme;

impl ProofSchemeFactory for StableOnlyProofScheme {
    fn descriptor(&self) -> SchemeDescriptor {
        StableOnlyRuntimeScheme.descriptor()
    }

    fn name(&self) -> &str {
        "stable_only"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        Ok(Arc::new(StableOnlyProofColumn { plan: plan.clone() }))
    }

    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        Ok(Arc::new(StableOnlyProofPreparer { plan: plan.clone() }))
    }
}

fn custom_compiled_program() -> tabula_compiler::SealedProgram {
    let schema = TableSchema {
        id: TableId(1),
        name: "accounts".to_string(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "balance".to_string(),
            value_type: ValueType::U64,
        }],
    };
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "noop".to_string(),
        param_schema: vec![],
        body: vec![],
    };
    let compiled = register_program(&[schema], &[tx]).expect("register program");
    let mut artifact = compiled.into_artifact();
    artifact.column_proof_plan[0].scheme_id = STABLE_ONLY_SCHEME_ID;
    artifact.column_proof_plan[0].scheme_descriptor = StableOnlyRuntimeScheme.descriptor();
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.column_proof_plan,
        )
        .expect("semantic hash"),
    );
    compiled_program_from_artifact(&artifact)
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

#[test]
fn stable_runtime_custom_scheme_surface_registers_via_bundle() {
    TabulaRuntime::builder(custom_compiled_program())
        .without_default_schemes()
        .with_scheme_bundle(
            SchemeBundle::new(StableOnlyRuntimeScheme, StableOnlyProofScheme)
                .expect("stable-only scheme bundle"),
        )
        .expect("register stable custom scheme bundle")
        .build()
        .expect("runtime should build through bundle-only surface");
}
