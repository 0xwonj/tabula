use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;
use tabula_ext::{
    ColumnBackendFactory, ColumnBackendFactoryBundle, PrecompileBackendFactory,
    PrecompileBackendFactoryBundle,
};
use tabula_ir::PrecompileId;

use crate::error::RuntimeError;

use super::default_backend_factories;

pub(crate) type SchemeFactoryMap = BTreeMap<SchemeId, Arc<dyn ColumnBackendFactory>>;
pub(crate) type PrecompileFactoryMap = BTreeMap<PrecompileId, Arc<dyn PrecompileBackendFactory>>;

/// Installed column backend factories available to one host process.
#[derive(Clone)]
pub struct InstalledSchemes {
    factories: SchemeFactoryMap,
}

impl InstalledSchemes {
    /// Seed the built-in SMT/SSMC backend factories.
    pub fn standard() -> Self {
        Self {
            factories: default_backend_factories(),
        }
    }

    /// Start with no installed scheme backends.
    pub fn empty() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    /// Register one installed scheme backend family.
    pub fn register_factory_arc(
        &mut self,
        factory: Arc<dyn ColumnBackendFactory>,
    ) -> Result<(), RuntimeError> {
        let scheme_id = factory.scheme_id();
        if self.factories.contains_key(&scheme_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate scheme backend registration for id {}",
                    scheme_id.0,
                ),
            });
        }
        self.factories.insert(scheme_id, factory);
        Ok(())
    }

    /// Consume and register one canonical backend bundle.
    pub fn with_column_backend_bundle(
        mut self,
        bundle: ColumnBackendFactoryBundle,
    ) -> Result<Self, RuntimeError> {
        self.register_factory_arc(bundle.into_factory())?;
        Ok(self)
    }

    pub(crate) fn factories(&self) -> &SchemeFactoryMap {
        &self.factories
    }
}

impl Default for InstalledSchemes {
    fn default() -> Self {
        Self::standard()
    }
}

/// Installed precompile backend families available to one host process.
#[derive(Clone, Default)]
pub struct InstalledPrecompiles {
    factories: PrecompileFactoryMap,
}

impl InstalledPrecompiles {
    /// Start with no installed precompile backends.
    pub fn empty() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    /// Register one installed precompile backend family.
    pub fn register_factory_arc(
        &mut self,
        factory: Arc<dyn PrecompileBackendFactory>,
    ) -> Result<(), RuntimeError> {
        let precompile_id = factory.precompile_id();
        if self.factories.contains_key(&precompile_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate precompile backend registration for id 0x{:04x}",
                    precompile_id.0,
                ),
            });
        }
        self.factories.insert(precompile_id, factory);
        Ok(())
    }

    /// Consume and register one canonical precompile backend bundle.
    pub fn with_precompile_backend_bundle(
        mut self,
        bundle: PrecompileBackendFactoryBundle,
    ) -> Result<Self, RuntimeError> {
        self.register_factory_arc(bundle.into_factory())?;
        Ok(self)
    }

    pub(crate) fn factories(&self) -> &PrecompileFactoryMap {
        &self.factories
    }
}
