use std::sync::Arc;

use tabula_artifact::SchemeDescriptor;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, ColumnLayoutKind, RootProfileId, SchemeId, TableId};
use tabula_machine::prelude::{ChipIdAllocator, DynChip};
use tabula_machine::{ChipExtension, ColumnChipSet, ProofColumn, SetupError};

use crate::columns::{
    ColumnPlan, ColumnProofInput, ColumnSchemeFactory, ColumnTransitionBackend,
    ColumnTransitionInput, ColumnViews, RuntimeColumn,
};

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

pub(crate) struct EmptyRuntimeColumn;

impl RuntimeColumn for EmptyRuntimeColumn {
    fn name(&self) -> &str {
        "empty"
    }
}

pub(crate) struct EmptyProofColumn {
    pub(crate) plan: ColumnPlan,
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

pub(crate) struct EmptyTransitionBackend {
    plan: ColumnPlan,
}

impl EmptyTransitionBackend {
    pub(crate) fn new(plan: ColumnPlan) -> Result<Self, SetupError> {
        if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SSMC_V1 {
            return Err(SetupError::SetupFailed(format!(
                "unsupported transition layout {}",
                plan.scheme_descriptor.layout_kind.0,
            )));
        }
        Ok(Self { plan })
    }
}

impl ColumnTransitionBackend for EmptyTransitionBackend {
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

    fn build_proof_input(
        &self,
        _input: ColumnTransitionInput,
        _property_reads: &[tabula_witness::trace::builtin::PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        Err(TabulaError::ProofError {
            phase: "runtime_builder_test",
            detail: "empty transition backend should not be used in prove flow".to_string(),
        })
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

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        Ok(ColumnViews::new(
            Arc::new(EmptyRuntimeColumn),
            Arc::new(EmptyProofColumn { plan: plan.clone() }),
            Arc::new(EmptyTransitionBackend::new(plan)?),
        ))
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

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        Ok(ColumnViews::new(
            Arc::new(EmptyRuntimeColumn),
            Arc::new(EmptyProofColumn { plan: plan.clone() }),
            Arc::new(EmptyTransitionBackend::new(plan)?),
        ))
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
    pub(crate) plan: ColumnPlan,
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

pub(crate) struct UnsupportedPropertyTransitionBackend {
    plan: ColumnPlan,
}

impl ColumnTransitionBackend for UnsupportedPropertyTransitionBackend {
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

    fn build_proof_input(
        &self,
        _input: ColumnTransitionInput,
        _property_reads: &[tabula_witness::trace::builtin::PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError> {
        Err(TabulaError::ProofError {
            phase: "runtime_builder_test",
            detail: "unsupported transition backend should not be used in prove flow".to_string(),
        })
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

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        if !plan.required_property_query_kinds.is_empty() {
            return Err(SetupError::SetupFailed(
                "unsupported property query".to_string(),
            ));
        }
        Ok(ColumnViews::new(
            Arc::new(UnsupportedPropertyRuntimeColumn),
            Arc::new(UnsupportedPropertyProofColumn { plan: plan.clone() }),
            Arc::new(UnsupportedPropertyTransitionBackend { plan }),
        ))
    }
}

pub(crate) struct DummyVerifierExtension;

impl ChipExtension for DummyVerifierExtension {
    fn name(&self) -> &str {
        "dummy_verifier"
    }

    fn airs(&self) -> Vec<Box<dyn tabula_machine::AnyRap>> {
        vec![]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![]
    }
}
