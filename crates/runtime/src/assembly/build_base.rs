use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;
use tabula_machine::{ChipExtension, MachineBuilder, RootProof, TabulaStarkConfig};

use crate::columns::{ColumnSchemeFactory, default_factories};
use crate::error::RuntimeError;

/// Shared machine/scheme build state used by runtime and verifier builders.
pub(crate) struct BuildBase {
    machine_builder: MachineBuilder,
    scheme_factories: BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
}

impl BuildBase {
    pub(crate) fn new() -> Self {
        Self {
            machine_builder: MachineBuilder::new(),
            scheme_factories: default_factories(),
        }
    }

    pub(crate) fn with_extension(mut self, ext: impl ChipExtension + 'static) -> Self {
        self.machine_builder = self.machine_builder.with_extension(ext);
        self
    }

    pub(crate) fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.machine_builder = self.machine_builder.with_root_proof(root);
        self
    }

    pub(crate) fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.machine_builder = self.machine_builder.with_config(config);
        self
    }

    pub(crate) fn with_scheme(
        mut self,
        factory: impl ColumnSchemeFactory + 'static,
    ) -> Result<Self, RuntimeError> {
        let scheme_id = factory.scheme_id();
        if self.scheme_factories.contains_key(&scheme_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate scheme factory registration for id {}",
                    scheme_id.0
                ),
            });
        }
        self.scheme_factories.insert(scheme_id, Arc::new(factory));
        Ok(self)
    }

    pub(crate) fn scheme_factories(&self) -> &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>> {
        &self.scheme_factories
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        MachineBuilder,
        BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
    ) {
        (self.machine_builder, self.scheme_factories)
    }
}
