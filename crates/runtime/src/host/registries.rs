//! Runtime type and encoding registries.

use std::sync::Arc;

use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry};

use crate::error::SetupError;

/// Host-owned runtime type and encoding implementations.
#[non_exhaustive]
#[derive(Clone)]
pub struct RuntimeRegistries {
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
}

impl RuntimeRegistries {
    /// Seed the built-in runtime type and encoding implementations.
    pub fn standard() -> Result<Self, SetupError> {
        Ok(Self {
            type_runtimes: TypeRuntimeRegistry::seeded().map_err(|error| {
                SetupError::Validation {
                    detail: error.to_string(),
                }
            })?,
            encoding_runtimes: EncodingRuntimeRegistry::seeded().map_err(|error| {
                SetupError::Validation {
                    detail: error.to_string(),
                }
            })?,
        })
    }

    /// Start with no runtime type or encoding implementations installed.
    pub fn empty() -> Self {
        Self {
            type_runtimes: TypeRuntimeRegistry::new(),
            encoding_runtimes: EncodingRuntimeRegistry::new(),
        }
    }

    /// Register one custom runtime type implementation.
    pub fn register_type_runtime(
        &mut self,
        runtime: Arc<dyn TypeRuntime>,
    ) -> Result<(), SetupError> {
        self.type_runtimes
            .register(runtime)
            .map_err(|err| SetupError::Validation {
                detail: err.to_string(),
            })
    }

    /// Register one custom runtime encoding implementation.
    pub fn register_encoding_runtime(
        &mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<(), SetupError> {
        self.encoding_runtimes
            .register(runtime)
            .map_err(|err| SetupError::Validation {
                detail: err.to_string(),
            })
    }

    /// Consume and register one runtime type implementation.
    pub fn with_type_runtime(
        mut self,
        runtime: impl TypeRuntime + 'static,
    ) -> Result<Self, SetupError> {
        self.register_type_runtime(Arc::new(runtime))?;
        Ok(self)
    }

    /// Consume and register one shared runtime type implementation.
    pub fn with_type_runtime_arc(
        mut self,
        runtime: Arc<dyn TypeRuntime>,
    ) -> Result<Self, SetupError> {
        self.register_type_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one runtime encoding implementation.
    pub fn with_encoding_runtime(
        mut self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, SetupError> {
        self.register_encoding_runtime(Arc::new(runtime))?;
        Ok(self)
    }

    /// Consume and register one shared runtime encoding implementation.
    pub fn with_encoding_runtime_arc(
        mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<Self, SetupError> {
        self.register_encoding_runtime(runtime)?;
        Ok(self)
    }

    /// Borrow the installed runtime type implementations.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.type_runtimes
    }

    /// Borrow the installed runtime encoding implementations.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.encoding_runtimes
    }
}
