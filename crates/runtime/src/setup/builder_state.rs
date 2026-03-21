use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::{RootProfileId, SchemeId};
use tabula_machine::{MachineBuilder, RootProof, TabulaStarkConfig};

use crate::columns::default_proof_factories;
#[cfg(feature = "prove")]
use crate::columns::{ColumnSchemeFactory, default_factories};
use crate::error::RuntimeError;
use crate::proof_extensions::ProofSchemeFactory;

#[cfg(feature = "prove")]
pub(crate) type RuntimeFactoryMap = BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>;
pub(crate) type ProofFactoryMap = BTreeMap<SchemeId, Arc<dyn ProofSchemeFactory>>;

/// Shared machine configuration state reused by runtime and verifier builders.
pub(crate) struct MachineConfigBase {
    machine_builder: MachineBuilder,
    root_profile_id: RootProfileId,
}

impl MachineConfigBase {
    pub(crate) fn new() -> Self {
        Self {
            machine_builder: MachineBuilder::new(),
            root_profile_id: RootProfileId::SMT_V1,
        }
    }

    pub(crate) fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.root_profile_id = root.profile_id();
        self.machine_builder = self.machine_builder.with_root_proof(root);
        self
    }

    pub(crate) fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.machine_builder = self.machine_builder.with_config(config);
        self
    }

    pub(crate) fn root_profile_id(&self) -> RootProfileId {
        self.root_profile_id
    }

    pub(crate) fn into_machine_builder(self) -> MachineBuilder {
        self.machine_builder
    }
}

/// Runtime-facing scheme registry. Builtins are just preloaded standard entries.
#[cfg(feature = "prove")]
pub(crate) struct RuntimeRegistryBase {
    factories: RuntimeFactoryMap,
}

#[cfg(feature = "prove")]
impl RuntimeRegistryBase {
    pub(crate) fn seeded() -> Self {
        Self {
            factories: default_factories(),
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn contains(&self, scheme_id: SchemeId) -> bool {
        self.factories.contains_key(&scheme_id)
    }

    pub(crate) fn insert_arc(
        &mut self,
        factory: Arc<dyn ColumnSchemeFactory>,
    ) -> Result<(), RuntimeError> {
        let scheme_id = factory.scheme_id();
        if self.factories.contains_key(&scheme_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate runtime scheme registration for id {}",
                    scheme_id.0
                ),
            });
        }
        self.factories.insert(scheme_id, factory);
        Ok(())
    }

    pub(crate) fn factories(&self) -> &RuntimeFactoryMap {
        &self.factories
    }
}

/// Proof-facing scheme registry. Builtins are just preloaded standard entries.
pub(crate) struct ProofRegistryBase {
    factories: ProofFactoryMap,
}

impl ProofRegistryBase {
    pub(crate) fn seeded() -> Self {
        Self {
            factories: default_proof_factories(),
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn contains(&self, scheme_id: SchemeId) -> bool {
        self.factories.contains_key(&scheme_id)
    }

    pub(crate) fn insert_arc(
        &mut self,
        factory: Arc<dyn ProofSchemeFactory>,
    ) -> Result<(), RuntimeError> {
        let scheme_id = factory.scheme_id();
        if self.factories.contains_key(&scheme_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!("duplicate proof scheme registration for id {}", scheme_id.0),
            });
        }
        self.factories.insert(scheme_id, factory);
        Ok(())
    }

    pub(crate) fn factories(&self) -> &ProofFactoryMap {
        &self.factories
    }
}
