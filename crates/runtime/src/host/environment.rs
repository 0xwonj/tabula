//! Host environment type bundling runtime registries and scheme backends.

use std::sync::Arc;

use tabula_ext::scheme::ColumnBackendFactoryBundle;
use tabula_types::{EncodingRuntime, TypeRuntime};

use crate::error::RuntimeError;

use super::{InstalledSchemes, RuntimeRegistries};

/// Host-owned runtime registries and scheme backends consumed by runtime and verifier builders.
#[non_exhaustive]
#[derive(Clone)]
pub struct HostEnvironment {
    runtime_registries: RuntimeRegistries,
    schemes: InstalledSchemes,
}

impl HostEnvironment {
    /// Seed the standard built-in host environment.
    pub fn standard() -> Result<Self, RuntimeError> {
        Ok(Self {
            runtime_registries: RuntimeRegistries::standard()?,
            schemes: InstalledSchemes::standard(),
        })
    }

    /// Start with no installed runtime registries or schemes.
    pub fn empty() -> Self {
        Self {
            runtime_registries: RuntimeRegistries::empty(),
            schemes: InstalledSchemes::empty(),
        }
    }

    /// Replace the installed runtime type/encoding implementations.
    pub fn with_runtime_registries(mut self, runtime_registries: RuntimeRegistries) -> Self {
        self.runtime_registries = runtime_registries;
        self
    }

    /// Replace the installed scheme backends.
    pub fn with_schemes(mut self, schemes: InstalledSchemes) -> Self {
        self.schemes = schemes;
        self
    }

    /// Consume and register one runtime type implementation.
    pub fn with_type_runtime(
        mut self,
        runtime: impl TypeRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.runtime_registries = self.runtime_registries.with_type_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one shared runtime type implementation.
    pub fn with_type_runtime_arc(
        mut self,
        runtime: Arc<dyn TypeRuntime>,
    ) -> Result<Self, RuntimeError> {
        self.runtime_registries = self.runtime_registries.with_type_runtime_arc(runtime)?;
        Ok(self)
    }

    /// Consume and register one runtime encoding implementation.
    pub fn with_encoding_runtime(
        mut self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.runtime_registries = self.runtime_registries.with_encoding_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one shared runtime encoding implementation.
    pub fn with_encoding_runtime_arc(
        mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<Self, RuntimeError> {
        self.runtime_registries = self.runtime_registries.with_encoding_runtime_arc(runtime)?;
        Ok(self)
    }

    /// Consume and register one canonical column backend bundle.
    pub fn with_column_backend_bundle(
        mut self,
        bundle: ColumnBackendFactoryBundle,
    ) -> Result<Self, RuntimeError> {
        self.schemes = self.schemes.with_column_backend_bundle(bundle)?;
        Ok(self)
    }

    /// Borrow the installed runtime type/encoding implementations.
    pub fn runtime_registries(&self) -> &RuntimeRegistries {
        &self.runtime_registries
    }

    /// Borrow the installed scheme backends.
    pub fn schemes(&self) -> &InstalledSchemes {
        &self.schemes
    }
}
