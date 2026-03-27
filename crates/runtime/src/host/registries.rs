//! Runtime type and encoding registries.

use std::sync::Arc;

use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry};

use crate::error::RuntimeError;

/// Host-owned runtime type and encoding implementations.
#[derive(Clone)]
pub struct RuntimeRegistries {
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
}

impl RuntimeRegistries {
    /// Seed the built-in runtime type and encoding implementations.
    pub fn standard() -> Self {
        Self {
            type_runtimes: TypeRuntimeRegistry::seeded()
                .expect("built-in type runtimes must remain valid"),
            encoding_runtimes: EncodingRuntimeRegistry::seeded()
                .expect("built-in encoding runtimes must remain valid"),
        }
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
    ) -> Result<(), RuntimeError> {
        self.type_runtimes
            .register(runtime)
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: err.to_string(),
            })
    }

    /// Register one custom runtime encoding implementation.
    pub fn register_encoding_runtime(
        &mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<(), RuntimeError> {
        self.encoding_runtimes
            .register(runtime)
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: err.to_string(),
            })
    }

    /// Consume and register one runtime type implementation.
    pub fn with_type_runtime(
        mut self,
        runtime: impl TypeRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.register_type_runtime(Arc::new(runtime))?;
        Ok(self)
    }

    /// Consume and register one shared runtime type implementation.
    pub fn with_type_runtime_arc(
        mut self,
        runtime: Arc<dyn TypeRuntime>,
    ) -> Result<Self, RuntimeError> {
        self.register_type_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one runtime encoding implementation.
    pub fn with_encoding_runtime(
        mut self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.register_encoding_runtime(Arc::new(runtime))?;
        Ok(self)
    }

    /// Consume and register one shared runtime encoding implementation.
    pub fn with_encoding_runtime_arc(
        mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<Self, RuntimeError> {
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

impl Default for RuntimeRegistries {
    fn default() -> Self {
        Self::standard()
    }
}
