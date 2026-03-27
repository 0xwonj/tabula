use std::sync::atomic::{AtomicU64, Ordering};

use tabula_compiler::{CompilerCatalogError, CompilerCatalogs};
use tabula_profile::SemanticRegistry;
use tabula_runtime::{HostEnvironment, RuntimeRegistries};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(feature = "execute")]
use tabula_machine::TabulaStarkConfig;

use crate::environment::{Environment, EnvironmentInner};
use crate::error::{InstallError, SdkError};
use crate::sdk::Sdk;

pub(crate) static NEXT_ENVIRONMENT_FINGERPRINT: AtomicU64 = AtomicU64::new(1);

/// Fluent builder for [`Sdk`].
pub struct SdkBuilder {
    compiler_catalogs: CompilerCatalogs,
    host_environment: HostEnvironment,
    #[cfg(feature = "execute")]
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
}

impl SdkBuilder {
    pub(crate) fn new() -> Self {
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
