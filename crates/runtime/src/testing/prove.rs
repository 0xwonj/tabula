use std::sync::Arc;

use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tabula_artifact::{Artifact, SchemeDescriptor};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, ColumnLayoutKind, RootProfileId, SchemeId, TableId};
use tabula_ext::backend::scheme::{
    ColumnProofContext, ColumnProofPreparer, PreparedColumnProof, ProofSchemeFactory,
};
use tabula_ext::backend::{ChipIdAllocator, ColumnChipSet, ProofColumn, SetupError};
use tabula_ext::{ColumnSchemeFactory, ExtError, ResolvedColumnPlan, RuntimeColumn};

const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";

pub(crate) fn custom_descriptor(scheme_id: SchemeId) -> SchemeDescriptor {
    SchemeDescriptor {
        scheme_id,
        scheme_version: 1,
        layout_kind: ColumnLayoutKind::SSMC_V1,
        params_hash: [scheme_id.raw() as u8; 32],
        root_profile_id: RootProfileId::SMT_V1,
        supported_property_query_kinds: vec![],
    }
}

pub(crate) fn unsupported_layout_descriptor(scheme_id: SchemeId) -> SchemeDescriptor {
    SchemeDescriptor {
        scheme_id,
        scheme_version: 1,
        layout_kind: ColumnLayoutKind(0x9000),
        params_hash: [scheme_id.raw() as u8; 32],
        root_profile_id: RootProfileId::SMT_V1,
        supported_property_query_kinds: vec![],
    }
}

pub(crate) fn set_artifact_column_scheme(
    artifact: &mut Artifact,
    index: usize,
    descriptor: SchemeDescriptor,
) {
    artifact.column_proof_plan[index].scheme_id = descriptor.scheme_id;
    artifact.column_proof_plan[index].scheme_descriptor = descriptor;
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.column_proof_plan,
        )
        .expect("semantic hash"),
    );
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

pub(crate) struct EmptyRuntimeColumn;

impl RuntimeColumn for EmptyRuntimeColumn {
    fn name(&self) -> &str {
        "empty"
    }
}

pub(crate) struct EmptyProofColumn {
    pub(crate) plan: ResolvedColumnPlan,
}

impl ProofColumn for EmptyProofColumn {
    fn name(&self) -> &str {
        "empty"
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

pub(crate) struct EmptyProofPreparer {
    pub(crate) plan: ResolvedColumnPlan,
}

impl ColumnProofPreparer for EmptyProofPreparer {
    fn name(&self) -> &str {
        "empty"
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn prepare_column(
        &self,
        _context: ColumnProofContext,
    ) -> Result<PreparedColumnProof, ExtError> {
        Err(ExtError::proof_preparation(TabulaError::ProofError {
            phase: "runtime_builder_test",
            detail: "empty proof preparer should not be used in prove flow".to_string(),
        }))
    }
}

pub(crate) struct EmptySchemeFactory;

impl ColumnSchemeFactory for EmptySchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor(SchemeId(0x1000))
    }

    fn name(&self) -> &str {
        "empty"
    }

    fn build_runtime_column(
        &self,
        _plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        Ok(Arc::new(EmptyRuntimeColumn))
    }
}

impl ProofSchemeFactory for EmptySchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor(SchemeId(0x1000))
    }

    fn name(&self) -> &str {
        "empty"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        Ok(Arc::new(EmptyProofColumn { plan: plan.clone() }))
    }

    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        Ok(Arc::new(EmptyProofPreparer { plan: plan.clone() }))
    }
}

pub(crate) struct UnsupportedLayoutSchemeFactory;

impl ColumnSchemeFactory for UnsupportedLayoutSchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        unsupported_layout_descriptor(SchemeId(0x1000))
    }

    fn name(&self) -> &str {
        "unsupported_layout"
    }

    fn build_runtime_column(
        &self,
        _plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        Err(ExtError::validation("unsupported proof scheme layout"))
    }
}

impl ProofSchemeFactory for UnsupportedLayoutSchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        unsupported_layout_descriptor(SchemeId(0x1000))
    }

    fn name(&self) -> &str {
        "unsupported_layout"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        let _ = plan;
        Err(ExtError::validation("unsupported proof scheme layout"))
    }

    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        Ok(Arc::new(EmptyProofPreparer { plan: plan.clone() }))
    }
}

#[derive(Clone)]
pub(crate) struct UnsupportedPropertyRuntimeColumn;

impl RuntimeColumn for UnsupportedPropertyRuntimeColumn {
    fn name(&self) -> &str {
        "unsupported"
    }
}

pub(crate) struct UnsupportedPropertyProofColumn {
    pub(crate) plan: ResolvedColumnPlan,
}

impl ProofColumn for UnsupportedPropertyProofColumn {
    fn name(&self) -> &str {
        "unsupported"
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

pub(crate) struct UnsupportedPropertyProofPreparer {
    plan: ResolvedColumnPlan,
}

impl ColumnProofPreparer for UnsupportedPropertyProofPreparer {
    fn name(&self) -> &str {
        "unsupported"
    }

    fn scheme_id(&self) -> SchemeId {
        self.plan.scheme_id
    }

    fn prepare_column(
        &self,
        _context: ColumnProofContext,
    ) -> Result<PreparedColumnProof, ExtError> {
        Err(ExtError::proof_preparation(TabulaError::ProofError {
            phase: "runtime_builder_test",
            detail: "unsupported proof preparer should not be used in prove flow".to_string(),
        }))
    }
}

pub(crate) struct UnsupportedPropertySchemeFactory;

impl ColumnSchemeFactory for UnsupportedPropertySchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor(SchemeId(0x1001))
    }

    fn name(&self) -> &str {
        "unsupported"
    }

    fn build_runtime_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn RuntimeColumn>, ExtError> {
        if !plan.required_property_query_kinds.is_empty() {
            return Err(ExtError::validation("unsupported property query"));
        }
        Ok(Arc::new(UnsupportedPropertyRuntimeColumn))
    }
}

impl ProofSchemeFactory for UnsupportedPropertySchemeFactory {
    fn descriptor(&self) -> SchemeDescriptor {
        custom_descriptor(SchemeId(0x1001))
    }

    fn name(&self) -> &str {
        "unsupported"
    }

    fn build_proof_column(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ProofColumn>, ExtError> {
        Ok(Arc::new(UnsupportedPropertyProofColumn {
            plan: plan.clone(),
        }))
    }

    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> Result<Arc<dyn ColumnProofPreparer>, ExtError> {
        Ok(Arc::new(UnsupportedPropertyProofPreparer {
            plan: plan.clone(),
        }))
    }
}
