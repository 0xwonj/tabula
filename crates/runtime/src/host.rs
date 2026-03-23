use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;
use tabula_ext::{
    ColumnBackendFactory, ColumnBackendFactoryBundle, PrecompileBackendFactory,
    PrecompileBackendFactoryBundle,
};
use tabula_ir::PrecompileId;
use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypeRuntime, TypeRuntimeRegistry};

use crate::error::RuntimeError;
#[cfg(any(feature = "prove", feature = "verify"))]
use crate::schemes::default_backend_factories;

pub(crate) type SchemeFactoryMap = BTreeMap<SchemeId, Arc<dyn ColumnBackendFactory>>;
pub(crate) type PrecompileFactoryMap = BTreeMap<PrecompileId, Arc<dyn PrecompileBackendFactory>>;

/// Host-owned runtime type and encoding implementations.
#[derive(Clone)]
pub struct HostTypeRuntimes {
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
}

impl HostTypeRuntimes {
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

impl Default for HostTypeRuntimes {
    fn default() -> Self {
        Self::standard()
    }
}

/// Installed column backend factories available to one host process.
#[derive(Clone)]
pub struct InstalledSchemes {
    factories: SchemeFactoryMap,
}

impl InstalledSchemes {
    /// Seed the built-in SMT/SSMC backend factories.
    pub fn standard() -> Self {
        Self {
            #[cfg(any(feature = "prove", feature = "verify"))]
            factories: default_backend_factories(),
            #[cfg(not(any(feature = "prove", feature = "verify")))]
            factories: BTreeMap::new(),
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

/// Host-owned installed capability surface consumed by runtime and verifier builders.
#[derive(Clone)]
pub struct HostEnvironment {
    type_runtimes: HostTypeRuntimes,
    schemes: InstalledSchemes,
    precompiles: InstalledPrecompiles,
}

impl HostEnvironment {
    /// Seed the standard built-in host environment.
    pub fn standard() -> Self {
        Self {
            type_runtimes: HostTypeRuntimes::standard(),
            schemes: InstalledSchemes::standard(),
            precompiles: InstalledPrecompiles::empty(),
        }
    }

    /// Start with no installed host capabilities.
    pub fn empty() -> Self {
        Self {
            type_runtimes: HostTypeRuntimes::empty(),
            schemes: InstalledSchemes::empty(),
            precompiles: InstalledPrecompiles::empty(),
        }
    }

    /// Replace the installed runtime type/encoding implementations.
    pub fn with_type_runtimes(mut self, type_runtimes: HostTypeRuntimes) -> Self {
        self.type_runtimes = type_runtimes;
        self
    }

    /// Replace the installed scheme backends.
    pub fn with_schemes(mut self, schemes: InstalledSchemes) -> Self {
        self.schemes = schemes;
        self
    }

    /// Replace the installed precompile backends.
    pub fn with_precompiles(mut self, precompiles: InstalledPrecompiles) -> Self {
        self.precompiles = precompiles;
        self
    }

    /// Consume and register one runtime type implementation.
    pub fn with_type_runtime(
        mut self,
        runtime: impl TypeRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.type_runtimes = self.type_runtimes.with_type_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one shared runtime type implementation.
    pub fn with_type_runtime_arc(
        mut self,
        runtime: Arc<dyn TypeRuntime>,
    ) -> Result<Self, RuntimeError> {
        self.type_runtimes = self.type_runtimes.with_type_runtime_arc(runtime)?;
        Ok(self)
    }

    /// Consume and register one runtime encoding implementation.
    pub fn with_encoding_runtime(
        mut self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, RuntimeError> {
        self.type_runtimes = self.type_runtimes.with_encoding_runtime(runtime)?;
        Ok(self)
    }

    /// Consume and register one shared runtime encoding implementation.
    pub fn with_encoding_runtime_arc(
        mut self,
        runtime: Arc<dyn EncodingRuntime>,
    ) -> Result<Self, RuntimeError> {
        self.type_runtimes = self.type_runtimes.with_encoding_runtime_arc(runtime)?;
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

    /// Consume and register one canonical precompile backend bundle.
    pub fn with_precompile_backend_bundle(
        mut self,
        bundle: PrecompileBackendFactoryBundle,
    ) -> Result<Self, RuntimeError> {
        self.precompiles = self.precompiles.with_precompile_backend_bundle(bundle)?;
        Ok(self)
    }

    /// Borrow the installed runtime type/encoding implementations.
    pub fn type_runtimes(&self) -> &HostTypeRuntimes {
        &self.type_runtimes
    }

    /// Borrow the installed scheme backends.
    pub fn schemes(&self) -> &InstalledSchemes {
        &self.schemes
    }

    /// Borrow the installed precompile backends.
    pub fn precompiles(&self) -> &InstalledPrecompiles {
        &self.precompiles
    }
}

impl Default for HostEnvironment {
    fn default() -> Self {
        Self::standard()
    }
}
