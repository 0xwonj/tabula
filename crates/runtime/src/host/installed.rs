//! Installed column backend scheme factories.

use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;
use tabula_ext::scheme::{ColumnBackendFactory, ColumnBackendFactoryBundle};

use crate::error::{RuntimeError, SetupError};

use super::default_backend_factories;

pub(crate) type SchemeFactoryMap = BTreeMap<SchemeId, Arc<dyn ColumnBackendFactory>>;

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
            return Err(SetupError::Validation {
                detail: format!(
                    "duplicate scheme backend registration for id {}",
                    scheme_id.0,
                ),
            }
            .into());
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
