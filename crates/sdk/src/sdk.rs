use std::sync::Arc;

use tabula_artifact::{Artifact, PrecompileDescriptor};
#[cfg(feature = "prove")]
use tabula_compiler::SealedProgram;
use tabula_compiler::{
    CompilerCatalogError, CompilerCatalogs, ProgramDefinition,
    compile_program_source_with_catalogs, register_artifact,
    register_program_definition_with_catalogs,
};
use tabula_profile::SemanticRegistry;
use tabula_runtime::{HostEnvironment, HostTypeRuntimes};
use tabula_types::{EncodingRuntime, TypeRuntime, TypeRuntimeRegistry};

#[cfg(feature = "verify")]
use tabula_runtime::MachineConfig;

#[cfg(feature = "verify")]
use tabula_ext::{ColumnBackendFactoryBundle, PrecompileBackendFactoryBundle};

use crate::error::SdkError;
use crate::program::Program;
#[cfg(feature = "verify")]
use crate::verifier::Verifier;

/// Shared process-level SDK context.
#[derive(Clone)]
pub struct Sdk {
    pub(crate) inner: Arc<SdkInner>,
}

/// Fluent builder for [`Sdk`].
pub struct SdkBuilder {
    compiler_catalogs: CompilerCatalogs,
    host_environment: HostEnvironment,
    #[cfg(feature = "verify")]
    machine_config: MachineConfig,
}

pub(crate) struct SdkInner {
    pub(crate) compiler_catalogs: CompilerCatalogs,
    pub(crate) host_environment: HostEnvironment,
    #[cfg(feature = "verify")]
    pub(crate) machine_config: MachineConfig,
}

impl SdkInner {
    pub(crate) fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        self.host_environment.type_runtimes().type_runtimes()
    }
}

impl Sdk {
    /// Build a standard SDK with built-in compiler catalogs, host environment, and machine config.
    pub fn standard() -> Self {
        Self::builder().build()
    }

    /// Start a customized SDK builder.
    pub fn builder() -> SdkBuilder {
        SdkBuilder::new()
    }

    /// Compile `.tab` source into a reusable SDK program.
    pub fn compile(&self, source: &str) -> Result<Program, SdkError> {
        let definition =
            compile_program_source_with_catalogs(source, &self.inner.compiler_catalogs)?;
        self.register(&definition)
    }

    /// Register an already-compiled program definition into a reusable SDK program.
    pub fn register(&self, definition: &ProgramDefinition) -> Result<Program, SdkError> {
        let compiled =
            register_program_definition_with_catalogs(definition, &self.inner.compiler_catalogs)?;
        Program::from_compiled(self.clone(), compiled)
    }

    /// Open a sealed artifact and validate it eagerly.
    pub fn open(&self, artifact: Artifact) -> Result<Program, SdkError> {
        let compiled = register_artifact(&artifact)?;
        Program::from_artifact(self.clone(), compiled, artifact)
    }

    /// Create an artifact-bound verifier that reuses this SDK context.
    #[cfg(feature = "verify")]
    pub fn verifier(&self, artifact: Artifact) -> Result<Verifier, SdkError> {
        let _compiled = register_artifact(&artifact)?;
        Ok(Verifier::new(self.clone(), artifact))
    }

    #[cfg(feature = "prove")]
    pub(crate) fn build_runtime(
        &self,
        sealed_program: &SealedProgram,
    ) -> Result<tabula_runtime::TabulaRuntime, SdkError> {
        tabula_runtime::TabulaRuntime::builder(sealed_program.clone())
            .with_host_environment(self.inner.host_environment.clone())
            .with_machine_config(self.inner.machine_config.clone())
            .build()
            .map_err(SdkError::from)
    }

    #[cfg(feature = "verify")]
    pub(crate) fn build_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::Verifier, SdkError> {
        tabula_runtime::Verifier::builder(artifact.clone())
            .with_host_environment(self.inner.host_environment.clone())
            .with_machine_config(self.inner.machine_config.clone())
            .build()
            .map_err(SdkError::from)
    }
}

impl SdkBuilder {
    fn new() -> Self {
        Self {
            compiler_catalogs: CompilerCatalogs::standard(),
            host_environment: HostEnvironment::standard(),
            #[cfg(feature = "verify")]
            machine_config: MachineConfig::standard(),
        }
    }

    /// Replace the compiler-owned sealing catalogs.
    pub fn with_compiler_catalogs(mut self, compiler_catalogs: CompilerCatalogs) -> Self {
        self.compiler_catalogs = compiler_catalogs;
        self
    }

    /// Replace the host-installed execution/proving environment.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Replace the machine-side proving and verification configuration.
    #[cfg(feature = "verify")]
    pub fn with_machine_config(mut self, machine_config: MachineConfig) -> Self {
        self.machine_config = machine_config;
        self
    }

    /// Register one custom canonical backend bundle.
    #[cfg(feature = "verify")]
    pub fn with_column_backend(
        mut self,
        bundle: ColumnBackendFactoryBundle,
    ) -> Result<Self, SdkError> {
        self.host_environment = self
            .host_environment
            .with_column_backend_bundle(bundle)
            .map_err(|err| SdkError::InvalidColumnBackendBundle {
                detail: err.to_string(),
            })?;
        Ok(self)
    }

    /// Replace the source authoring/sealing semantic registry.
    pub fn with_semantic_registry(mut self, semantics: SemanticRegistry) -> Result<Self, SdkError> {
        self.compiler_catalogs = self
            .compiler_catalogs
            .with_semantic_registry(semantics)
            .map_err(map_compiler_catalog_error)?;
        Ok(self)
    }

    /// Clear all seeded runtime type and encoding implementations.
    pub fn without_default_types(mut self) -> Self {
        self.host_environment = self
            .host_environment
            .with_type_runtimes(HostTypeRuntimes::empty());
        self
    }

    /// Register one custom runtime type implementation.
    pub fn with_type_runtime(
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

    /// Register one custom runtime encoding implementation.
    pub fn with_encoding_runtime(
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

    /// Register one precompile descriptor available during source-level sealing.
    pub fn with_precompile_descriptor(
        mut self,
        descriptor: PrecompileDescriptor,
    ) -> Result<Self, SdkError> {
        self.compiler_catalogs = self
            .compiler_catalogs
            .with_precompile_descriptor(descriptor)
            .map_err(map_compiler_catalog_error)?;
        Ok(self)
    }

    /// Register one installed precompile backend family.
    #[cfg(feature = "verify")]
    pub fn with_precompile_backend(
        mut self,
        bundle: PrecompileBackendFactoryBundle,
    ) -> Result<Self, SdkError> {
        self.host_environment = self
            .host_environment
            .with_precompile_backend_bundle(bundle)
            .map_err(|err| SdkError::InvalidPrecompileBackendBundle {
                detail: err.to_string(),
            })?;
        Ok(self)
    }

    /// Register both the compiler-visible descriptor and the host-installed backend family.
    #[cfg(feature = "verify")]
    pub fn with_precompile_support(
        mut self,
        descriptor: PrecompileDescriptor,
        bundle: PrecompileBackendFactoryBundle,
    ) -> Result<Self, SdkError> {
        if bundle.precompile_id() != descriptor.precompile_id {
            return Err(SdkError::InvalidPrecompileBackendBundle {
                detail: format!(
                    "precompile support bundle id 0x{:04x} does not match descriptor id 0x{:04x}",
                    bundle.precompile_id().0,
                    descriptor.precompile_id.0,
                ),
            });
        }
        self = self.with_precompile_descriptor(descriptor)?;
        self = self.with_precompile_backend(bundle)?;
        Ok(self)
    }

    /// Override the root proof backend used by runtime and verifier builders.
    #[cfg(feature = "verify")]
    pub fn with_root_proof(mut self, root: impl tabula_machine::RootProof + 'static) -> Self {
        self.machine_config = self.machine_config.with_root_proof(root);
        self
    }

    /// Override the STARK configuration used by runtime and verifier builders.
    #[cfg(feature = "verify")]
    pub fn with_machine_stark_config(mut self, config: tabula_machine::TabulaStarkConfig) -> Self {
        self.machine_config = self.machine_config.with_config(config);
        self
    }

    /// Finalize the SDK configuration.
    pub fn build(self) -> Sdk {
        Sdk {
            inner: Arc::new(SdkInner {
                compiler_catalogs: self.compiler_catalogs,
                host_environment: self.host_environment,
                #[cfg(feature = "verify")]
                machine_config: self.machine_config,
            }),
        }
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
        f.debug_struct("Sdk").finish_non_exhaustive()
    }
}

fn map_compiler_catalog_error(err: CompilerCatalogError) -> SdkError {
    match err {
        CompilerCatalogError::InvalidSemanticRegistry(err) => SdkError::InvalidSemanticRegistry {
            detail: err.to_string(),
        },
        CompilerCatalogError::DuplicatePrecompileDescriptor { precompile_id } => {
            SdkError::InvalidPrecompileDescriptorRegistration {
                detail: format!(
                    "duplicate precompile descriptor registration for id 0x{:04x}",
                    precompile_id.0,
                ),
            }
        }
        CompilerCatalogError::InvalidPrecompileDescriptor { detail } => {
            SdkError::InvalidPrecompileDescriptorRegistration { detail }
        }
    }
}
