use std::sync::Arc;

pub use tabula_compiler::{
    CompileDiagnostic, CompiledProgram, CompilerCatalogs, RegisteredProgram,
    SourceCapabilityDescriptor, StateFieldSchemeBinding,
};
use tabula_profile::SemanticRegistry;
use tabula_runtime::{HostEnvironment, RuntimeRegistries};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "execute")]
use tabula_executor::ExecutionJournal;
#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(feature = "execute")]
pub use tabula_ext::scheme::ColumnBackendFactoryBundle;
#[cfg(feature = "execute")]
use tabula_machine::TabulaStarkConfig;

use crate::{
    Artifact, Context, ExecutionReceipt, Sdk, SdkBuilder, SdkError, State, TransactionBatch,
};

pub use tabula_compiler;
pub use tabula_core::PortableValue;
pub use tabula_ir::{ContextInput, EntryBatch, EntryCall, EntryId, EntryKind, FieldId, TableId};
pub use tabula_runtime::{ProofStatement, StateSnapshot};

pub trait SdkBuilderExt {
    fn with_compiler_catalogs(self, compiler_catalogs: CompilerCatalogs) -> Self;
    fn with_host_environment(self, host_environment: HostEnvironment) -> Self;
    fn with_semantic_registry(self, semantics: SemanticRegistry) -> Result<Self, SdkError>
    where
        Self: Sized;
    fn with_capability_descriptor(
        self,
        descriptor: SourceCapabilityDescriptor,
    ) -> Result<Self, SdkError>
    where
        Self: Sized;
    fn without_default_types(self) -> Self;
    fn with_type_runtime(self, runtime: impl TypeRuntime + 'static) -> Result<Self, SdkError>
    where
        Self: Sized;
    fn with_encoding_runtime(
        self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, SdkError>
    where
        Self: Sized;
    #[cfg(feature = "execute")]
    fn with_column_backend(self, bundle: ColumnBackendFactoryBundle) -> Result<Self, SdkError>
    where
        Self: Sized;
    #[cfg(feature = "execute")]
    fn with_machine_stark_config(self, config: TabulaStarkConfig) -> Self;
    #[cfg(feature = "prove")]
    fn with_root_backend_bundle(self, bundle: RootBackendBundle) -> Self;
}

impl SdkBuilderExt for SdkBuilder {
    fn with_compiler_catalogs(self, compiler_catalogs: CompilerCatalogs) -> Self {
        self.with_compiler_catalogs_internal(compiler_catalogs)
    }

    fn with_host_environment(self, host_environment: HostEnvironment) -> Self {
        self.with_host_environment_internal(host_environment)
    }

    fn with_semantic_registry(self, semantics: SemanticRegistry) -> Result<Self, SdkError> {
        self.with_semantic_registry_internal(semantics)
    }

    fn with_capability_descriptor(
        self,
        descriptor: SourceCapabilityDescriptor,
    ) -> Result<Self, SdkError> {
        self.with_capability_descriptor_internal(descriptor)
    }

    fn without_default_types(self) -> Self {
        self.without_default_types_internal()
    }

    fn with_type_runtime(self, runtime: impl TypeRuntime + 'static) -> Result<Self, SdkError> {
        self.with_type_runtime_internal(runtime)
    }

    fn with_encoding_runtime(
        self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, SdkError> {
        self.with_encoding_runtime_internal(runtime)
    }

    #[cfg(feature = "execute")]
    fn with_column_backend(self, bundle: ColumnBackendFactoryBundle) -> Result<Self, SdkError> {
        self.with_column_backend_internal(bundle)
    }

    #[cfg(feature = "execute")]
    fn with_machine_stark_config(self, config: TabulaStarkConfig) -> Self {
        self.with_machine_stark_config_internal(config)
    }

    #[cfg(feature = "prove")]
    fn with_root_backend_bundle(self, bundle: RootBackendBundle) -> Self {
        self.with_root_backend_bundle_internal(bundle)
    }
}

pub trait ArtifactExt {
    fn registered_program(&self) -> &RegisteredProgram;
    fn clone_registered_program(&self) -> RegisteredProgram;
}

impl ArtifactExt for Artifact {
    fn registered_program(&self) -> &RegisteredProgram {
        self.registered()
    }

    fn clone_registered_program(&self) -> RegisteredProgram {
        self.registered().clone()
    }
}

pub trait StateExt {
    fn snapshot(&self) -> &StateSnapshot;
}

impl StateExt for State {
    fn snapshot(&self) -> &StateSnapshot {
        self.as_raw()
    }
}

pub trait ContextExt {
    fn input(&self) -> &ContextInput;
}

impl ContextExt for Context {
    fn input(&self) -> &ContextInput {
        self.as_raw()
    }
}

pub trait TransactionBatchExt {
    fn batch(&self) -> &EntryBatch;
}

impl TransactionBatchExt for TransactionBatch {
    fn batch(&self) -> &EntryBatch {
        self.as_raw()
    }
}

#[cfg(feature = "execute")]
pub trait ExecutionReceiptExt {
    fn journal(&self) -> &ExecutionJournal;
}

#[cfg(feature = "execute")]
impl ExecutionReceiptExt for ExecutionReceipt {
    fn journal(&self) -> &ExecutionJournal {
        &self.inner.journal
    }
}

pub fn register_compiled(sdk: &Sdk, compiled: CompiledProgram) -> Result<Artifact, SdkError> {
    sdk.register_compiled(compiled)
}

pub fn runtime_registries(environment: &crate::Environment) -> &RuntimeRegistries {
    environment.inner.host_environment.runtime_registries()
}

pub fn host_environment(environment: &crate::Environment) -> &HostEnvironment {
    &environment.inner.host_environment
}

pub fn compiler_catalogs(environment: &crate::Environment) -> &CompilerCatalogs {
    &environment.inner.compiler_catalogs
}

pub fn build_host_environment(
    runtime_registries: RuntimeRegistries,
    schemes: tabula_runtime::InstalledSchemes,
) -> HostEnvironment {
    HostEnvironment::empty()
        .with_runtime_registries(runtime_registries)
        .with_schemes(schemes)
}

pub fn share_type_runtime(runtime: impl TypeRuntime + 'static) -> Arc<dyn TypeRuntime> {
    Arc::new(runtime)
}

pub fn share_encoding_runtime(runtime: impl EncodingRuntime + 'static) -> Arc<dyn EncodingRuntime> {
    Arc::new(runtime)
}
