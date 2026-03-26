use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(any(feature = "execute", feature = "verify"))]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "compile")]
use tabula_compiler::compile_and_register_program_source;
use tabula_compiler::{
    CompiledProgram, CompilerCatalogError, CompilerCatalogs, register_compiled_program,
};
use tabula_profile::SemanticRegistry;
use tabula_runtime::{HostEnvironment, RuntimeRegistries};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "execute")]
use tabula_machine::TabulaStarkConfig;

#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;

use crate::artifact::Artifact;
use crate::environment::{Environment, EnvironmentInner};
use crate::error::{InstallError, SdkError};
use crate::program::Program;

static NEXT_ENVIRONMENT_FINGERPRINT: AtomicU64 = AtomicU64::new(1);

/// Shared process-level SDK context.
#[derive(Clone)]
pub struct Sdk {
    pub(crate) inner: Arc<SdkInner>,
}

/// Fluent builder for [`Sdk`].
pub struct SdkBuilder {
    compiler_catalogs: CompilerCatalogs,
    host_environment: HostEnvironment,
    #[cfg(feature = "execute")]
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
}

pub(crate) struct SdkInner {
    pub(crate) environment: Environment,
    #[cfg(feature = "execute")]
    pub(crate) runtime_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::TabulaRuntime>>>,
    #[cfg(feature = "verify")]
    pub(crate) verifier_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::Verifier>>>,
}

impl Sdk {
    /// Build a standard SDK with built-in compiler catalogs and host environment.
    pub fn standard() -> Self {
        Self::builder()
            .build()
            .expect("built-in SDK environment must remain installable")
    }

    /// Start a customized SDK builder.
    pub fn builder() -> SdkBuilder {
        SdkBuilder::new()
    }

    /// Build one SDK from an already prepared environment.
    pub fn from_environment(environment: Environment) -> Self {
        Self {
            inner: Arc::new(SdkInner {
                environment,
                #[cfg(feature = "execute")]
                runtime_cache: Mutex::new(BTreeMap::new()),
                #[cfg(feature = "verify")]
                verifier_cache: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// Borrow the immutable application environment bound to this SDK.
    pub fn environment(&self) -> &Environment {
        &self.inner.environment
    }

    /// Compile rewritten source into a sealed artifact.
    #[cfg(feature = "compile")]
    pub fn compile(&self, source: &str) -> Result<Artifact, SdkError> {
        let registered = compile_and_register_program_source(
            source,
            &self.inner.environment.inner.compiler_catalogs,
        )?;
        Artifact::from_registered(registered)
    }

    /// Load one serialized artifact payload from bytes.
    pub fn load_artifact(&self, bytes: &[u8]) -> Result<Artifact, SdkError> {
        serde_json::from_slice(bytes).map_err(|error| SdkError::ArtifactDecode {
            detail: error.to_string(),
        })
    }

    /// Open one artifact into a reusable semantic program handle.
    pub fn open(&self, artifact: Artifact) -> Result<Program, SdkError> {
        Ok(Program::new(self.clone(), artifact))
    }

    pub(crate) fn register_compiled(
        &self,
        compiled: CompiledProgram,
    ) -> Result<Artifact, SdkError> {
        let registered =
            register_compiled_program(compiled, &self.inner.environment.inner.compiler_catalogs)?;
        Artifact::from_registered(registered)
    }

    #[cfg(feature = "execute")]
    pub(crate) fn prepare_runtime(
        &self,
        artifact: &Artifact,
    ) -> Result<Arc<tabula_runtime::TabulaRuntime>, SdkError> {
        let key = self.cache_key("runner", artifact);
        let mut cache = self
            .inner
            .runtime_cache
            .lock()
            .expect("sdk runtime cache mutex poisoned");
        if let Some(runtime) = cache.get(&key) {
            return Ok(Arc::clone(runtime));
        }

        let built = Arc::new(self.build_runtime(artifact)?);
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "verify")]
    pub(crate) fn prepare_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<Arc<tabula_runtime::Verifier>, SdkError> {
        let key = self.cache_key("verifier", artifact);
        let mut cache = self
            .inner
            .verifier_cache
            .lock()
            .expect("sdk verifier cache mutex poisoned");
        if let Some(verifier) = cache.get(&key) {
            return Ok(Arc::clone(verifier));
        }

        let built = Arc::new(self.build_verifier(artifact)?);
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "execute")]
    fn build_runtime(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::TabulaRuntime, SdkError> {
        let builder = tabula_runtime::TabulaRuntime::builder(artifact.registered().clone())
            .with_host_environment(self.inner.environment.inner.host_environment.clone())
            .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone());
        #[cfg(feature = "prove")]
        let builder = builder
            .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
        builder.build().map_err(SdkError::from)
    }

    #[cfg(feature = "verify")]
    fn build_verifier(&self, artifact: &Artifact) -> Result<tabula_runtime::Verifier, SdkError> {
        let builder = tabula_runtime::Verifier::builder(artifact.registered().clone())
            .with_host_environment(self.inner.environment.inner.host_environment.clone())
            .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone());
        #[cfg(feature = "prove")]
        let builder = builder
            .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
        builder.build().map_err(SdkError::from)
    }

    fn cache_key(&self, mode: &str, artifact: &Artifact) -> String {
        format!(
            "{}:{}:{}",
            self.inner.environment.inner.fingerprint,
            artifact.digest(),
            mode
        )
    }
}

impl SdkBuilder {
    fn new() -> Self {
        Self {
            compiler_catalogs: CompilerCatalogs::standard(),
            host_environment: HostEnvironment::standard(),
            #[cfg(feature = "execute")]
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: RootBackendBundle::standard(),
        }
    }

    /// Install one extension bundle atomically.
    pub fn with_extension(mut self, extension: &tabula_ext::Extension) -> Result<Self, SdkError> {
        self.apply_extension(extension)?;
        Ok(self)
    }

    /// Finalize the environment without building an SDK wrapper.
    pub fn build_environment(self) -> Result<Environment, InstallError> {
        let fingerprint = NEXT_ENVIRONMENT_FINGERPRINT.fetch_add(1, Ordering::Relaxed);
        Ok(Environment::new(EnvironmentInner {
            compiler_catalogs: self.compiler_catalogs,
            host_environment: self.host_environment,
            fingerprint,
            #[cfg(feature = "execute")]
            machine_stark_config: self.machine_stark_config,
            #[cfg(feature = "prove")]
            root_backend_bundle: self.root_backend_bundle,
        }))
    }

    /// Finalize the SDK configuration.
    pub fn build(self) -> Result<Sdk, InstallError> {
        Ok(Sdk::from_environment(self.build_environment()?))
    }

    pub(crate) fn with_compiler_catalogs_internal(
        mut self,
        compiler_catalogs: CompilerCatalogs,
    ) -> Self {
        self.compiler_catalogs = compiler_catalogs;
        self
    }

    pub(crate) fn with_host_environment_internal(
        mut self,
        host_environment: HostEnvironment,
    ) -> Self {
        self.host_environment = host_environment;
        self
    }

    pub(crate) fn with_semantic_registry_internal(
        mut self,
        semantics: SemanticRegistry,
    ) -> Result<Self, SdkError> {
        self.compiler_catalogs = self
            .compiler_catalogs
            .with_semantic_registry(semantics)
            .map_err(map_compiler_catalog_error)?;
        Ok(self)
    }

    pub(crate) fn with_capability_descriptor_internal(
        mut self,
        descriptor: tabula_compiler::SourceCapabilityDescriptor,
    ) -> Result<Self, SdkError> {
        self.compiler_catalogs = self
            .compiler_catalogs
            .with_capability_descriptor(descriptor)
            .map_err(map_compiler_catalog_error)?;
        Ok(self)
    }

    pub(crate) fn without_default_types_internal(mut self) -> Self {
        self.host_environment = self
            .host_environment
            .with_runtime_registries(RuntimeRegistries::empty());
        self
    }

    pub(crate) fn with_type_runtime_internal(
        mut self,
        runtime: impl TypeRuntime + 'static,
    ) -> Result<Self, SdkError> {
        self.host_environment =
            self.host_environment
                .with_type_runtime(runtime)
                .map_err(|err| SdkError::InvalidSemanticRegistry {
                    detail: err.to_string(),
                })?;
        Ok(self)
    }

    pub(crate) fn with_encoding_runtime_internal(
        mut self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, SdkError> {
        self.host_environment = self
            .host_environment
            .with_encoding_runtime(runtime)
            .map_err(|err| SdkError::InvalidSemanticRegistry {
                detail: err.to_string(),
            })?;
        Ok(self)
    }

    #[cfg(feature = "execute")]
    pub(crate) fn with_column_backend_internal(
        mut self,
        bundle: tabula_ext::scheme::ColumnBackendFactoryBundle,
    ) -> Result<Self, SdkError> {
        self.host_environment = self
            .host_environment
            .with_column_backend_bundle(bundle)
            .map_err(|err| SdkError::InvalidColumnBackendBundle {
                detail: err.to_string(),
            })?;
        Ok(self)
    }

    #[cfg(feature = "execute")]
    pub(crate) fn with_machine_stark_config_internal(mut self, config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = config;
        self
    }

    #[cfg(feature = "prove")]
    pub(crate) fn with_root_backend_bundle_internal(
        mut self,
        root_backend_bundle: RootBackendBundle,
    ) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    fn apply_extension(&mut self, extension: &tabula_ext::Extension) -> Result<(), SdkError> {
        let mut semantics = self.compiler_catalogs.semantics().clone();
        for contribution in extension.types() {
            semantics
                .register_type_descriptor(contribution.descriptor().clone())
                .map_err(|error| SdkError::InvalidExtension {
                    detail: error.to_string(),
                })?;
            semantics
                .register_type_name(
                    contribution.source_name(),
                    contribution.descriptor().type_id,
                )
                .map_err(|error| SdkError::InvalidExtension {
                    detail: error.to_string(),
                })?;
        }
        for contribution in extension.encodings() {
            semantics
                .register_encoding_profile(contribution.profile().clone())
                .map_err(|error| SdkError::InvalidExtension {
                    detail: error.to_string(),
                })?;
            if let Some(type_id) = contribution.default_for_type() {
                semantics
                    .register_default_encoding(type_id, contribution.profile().encoding_profile_id)
                    .map_err(|error| SdkError::InvalidExtension {
                        detail: error.to_string(),
                    })?;
            }
        }
        for contribution in extension.schemes() {
            semantics
                .register_scheme_profile(contribution.profile().clone())
                .map_err(|error| SdkError::InvalidExtension {
                    detail: error.to_string(),
                })?;
            semantics
                .register_scheme_name(
                    contribution.source_name(),
                    contribution.profile().scheme_family_id,
                )
                .map_err(|error| SdkError::InvalidExtension {
                    detail: error.to_string(),
                })?;
            for encoding_profile_id in contribution.default_encodings() {
                semantics
                    .register_default_scheme_profile(
                        contribution.profile().scheme_family_id,
                        *encoding_profile_id,
                        contribution.profile().scheme_profile_id,
                    )
                    .map_err(|error| SdkError::InvalidExtension {
                        detail: error.to_string(),
                    })?;
            }
        }
        self.compiler_catalogs = self
            .compiler_catalogs
            .clone()
            .with_semantic_registry(semantics)
            .map_err(map_compiler_catalog_error)?;
        for contribution in extension.capabilities() {
            self.compiler_catalogs = self
                .compiler_catalogs
                .clone()
                .with_capability_descriptor(contribution.descriptor().clone())
                .map_err(map_compiler_catalog_error)?;
        }

        let mut host_environment = self.host_environment.clone();
        for contribution in extension.types() {
            host_environment = host_environment
                .with_type_runtime_arc(contribution.runtime())
                .map_err(SdkError::from)?;
        }
        for contribution in extension.encodings() {
            host_environment = host_environment
                .with_encoding_runtime_arc(contribution.runtime())
                .map_err(SdkError::from)?;
        }
        #[cfg(feature = "execute")]
        for contribution in extension.schemes() {
            host_environment = host_environment
                .with_column_backend_bundle(contribution.backend_bundle())
                .map_err(|error| SdkError::InvalidColumnBackendBundle {
                    detail: error.to_string(),
                })?;
        }
        self.host_environment = host_environment;

        #[cfg(feature = "prove")]
        if let Some(root_backend_bundle) = extension.root_backend_bundle() {
            self.root_backend_bundle = root_backend_bundle;
        }

        Ok(())
    }
}

impl Default for SdkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SdkBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SdkBuilder").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Sdk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sdk")
            .field("environment", &self.inner.environment)
            .finish_non_exhaustive()
    }
}

fn map_compiler_catalog_error(err: CompilerCatalogError) -> SdkError {
    match err {
        CompilerCatalogError::InvalidSemanticRegistry(err) => SdkError::InvalidSemanticRegistry {
            detail: err.to_string(),
        },
        CompilerCatalogError::DuplicateCapabilityDescriptor { path } => {
            SdkError::InvalidCapabilityDescriptorRegistration {
                detail: format!("duplicate capability descriptor registration for path {path}"),
            }
        }
        CompilerCatalogError::InvalidCapabilityDescriptor { detail } => {
            SdkError::InvalidCapabilityDescriptorRegistration { detail }
        }
    }
}
