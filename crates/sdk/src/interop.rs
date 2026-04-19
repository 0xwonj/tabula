//! Low-level interop helpers for advanced SDK integrations.
//!
//! This module exposes extension traits and free functions that give framework
//! code (host integrations, test harnesses) direct access to the underlying
//! runtime and compiler types that the high-level SDK otherwise hides.

use std::sync::Arc;

pub use tabula_compiler::{
    CompileDiagnostic, CompiledProgram, CompilerCatalogs, RegisteredProgram,
    SourceCapabilityDescriptor,
};
pub use tabula_core::{CommittedCellKey, CommittedKey, CommittedPropertyQuery, PortableValue};
use tabula_profile::SemanticRegistry;
use tabula_runtime::{HostEnvironment, RuntimeRegistries};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "execute")]
pub use tabula_executor::{
    CapabilityEffect, ExecutionJournal, ExecutionStateSummary, FailedTxExecution,
    QueryExecutionResult, SuccessfulTxExecution, TxExecutionOutcome, TypedStateSnapshot,
    TypedStateWrite,
};
#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(feature = "execute")]
pub use tabula_ext::scheme::ColumnBackendFactoryBundle;
#[cfg(feature = "execute")]
use tabula_machine::TabulaStarkConfig;
#[cfg(feature = "execute")]
pub use tabula_runtime::TabulaRuntime;
#[cfg(feature = "prove")]
pub use tabula_runtime::{PreparedProver, PreparedProverBuilder};
#[cfg(feature = "verify")]
pub use tabula_runtime::{PreparedVerifier, PreparedVerifierBuilder, VerifierState};
#[cfg(feature = "execute")]
pub use tabula_types::{
    RelationEffect, RelationEffectKind, StateEffectKind, StatePropertyEffect, TypedEventEffect,
    TypedStateEffect,
};
pub use tabula_types::{TypedCommittedPropertyQueryResult, TypedValue};

use crate::{
    Artifact, Context, ExecutionReceipt, Sdk, SdkBuilder, SdkError, State, TransactionBatch,
};

pub use tabula_compiler;
pub use tabula_contract::{
    BoundStatement, ProofEncodingId, ProofEnvelope, ProofSystemId, PublicStatement,
    decode_proof_envelope, encode_proof_envelope,
};
pub use tabula_ir::{
    CapabilityId, CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality,
    ContextFieldId, ContextInput, EntryBatch, EntryCall, EntryId, EntryKind, EventId, FieldId,
    HashFamily, RelationId, StatePropertyQuery, TableId, TypeRef,
};
pub use tabula_runtime::CommittedStateSnapshot;

/// Raw runtime-owned parts needed to construct one SDK execution receipt.
#[cfg(feature = "execute")]
pub struct RawExecutionReceiptParts {
    #[cfg(feature = "prove")]
    /// Canonical program digest carried by prove-capable receipts.
    pub program_digest: String,
    /// Logical state before execution.
    pub state_before: State,
    /// Exact committed snapshot before execution.
    pub snapshot: tabula_runtime::CommittedStateSnapshot,
    /// Executed transaction batch.
    pub batch: tabula_ir::EntryBatch,
    /// Context input supplied to execution.
    pub context: tabula_ir::ContextInput,
    /// Logical state after execution.
    pub state_after: State,
    /// Exact committed snapshot after execution.
    pub state_after_snapshot: tabula_runtime::CommittedStateSnapshot,
    /// Runtime execution journal captured during execution.
    pub journal: tabula_executor::ExecutionJournal,
}

/// Extension trait for [`SdkBuilder`] exposing advanced configuration options.
pub trait SdkBuilderExt {
    /// Override the compiler type and semantic catalogs.
    fn with_compiler_catalogs(self, compiler_catalogs: CompilerCatalogs) -> Self;
    /// Override the host environment (runtime registries and installed schemes).
    fn with_host_environment(self, host_environment: HostEnvironment) -> Self;
    /// Register a semantic registry (type definitions).
    fn with_semantic_registry(self, semantics: SemanticRegistry) -> Result<Self, SdkError>
    where
        Self: Sized;
    /// Register a source-level capability descriptor for the compiler.
    fn with_capability_descriptor(
        self,
        descriptor: SourceCapabilityDescriptor,
    ) -> Result<Self, SdkError>
    where
        Self: Sized;
    /// Remove the default built-in type registrations.
    fn without_default_types(self) -> Self;
    /// Register a custom [`TypeRuntime`] for encoding/decoding values.
    fn with_type_runtime(self, runtime: impl TypeRuntime + 'static) -> Result<Self, SdkError>
    where
        Self: Sized;
    /// Register a custom [`EncodingRuntime`] for portable value encoding.
    fn with_encoding_runtime(
        self,
        runtime: impl EncodingRuntime + 'static,
    ) -> Result<Self, SdkError>
    where
        Self: Sized;
    /// Register a column backend factory bundle for STARK column proofs.
    #[cfg(feature = "execute")]
    fn with_column_backend(self, bundle: ColumnBackendFactoryBundle) -> Result<Self, SdkError>
    where
        Self: Sized;
    /// Override the STARK configuration used when building the prover machine.
    #[cfg(feature = "execute")]
    fn with_machine_stark_config(self, config: TabulaStarkConfig) -> Self;
    /// Override the root proof backend bundle.
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

/// Extension trait for [`Artifact`] providing access to the underlying IR program.
pub trait ArtifactExt {
    /// Borrow the registered program backing the artifact.
    fn registered_program(&self) -> &RegisteredProgram;
    /// Clone the registered program.
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

/// Extension trait for [`State`] providing access to the raw state snapshot.
pub trait StateExt {
    /// Borrow the underlying logical keyed cells.
    fn cells(&self) -> &[crate::types::LogicalStateCell];
}

impl StateExt for State {
    fn cells(&self) -> &[crate::types::LogicalStateCell] {
        self.cells_raw()
    }
}

/// Extension trait for [`Context`] providing access to the raw context input.
pub trait ContextExt {
    /// Borrow the underlying [`ContextInput`].
    fn input(&self) -> &ContextInput;
}

impl ContextExt for Context {
    fn input(&self) -> &ContextInput {
        self.as_raw()
    }
}

/// Extension trait for [`TransactionBatch`] providing access to the raw entry batch.
pub trait TransactionBatchExt {
    /// Borrow the underlying [`EntryBatch`].
    fn batch(&self) -> &EntryBatch;
}

impl TransactionBatchExt for TransactionBatch {
    fn batch(&self) -> &EntryBatch {
        self.as_raw()
    }
}

/// Extension trait for [`ExecutionReceipt`] providing access to the raw execution journal.
#[cfg(feature = "execute")]
pub trait ExecutionReceiptExt {
    /// Borrow the committed pre-state snapshot used by the runtime.
    fn snapshot(&self) -> &CommittedStateSnapshot;
    /// Borrow the committed post-state snapshot produced by the runtime.
    fn state_after_snapshot(&self) -> &CommittedStateSnapshot;
    /// Borrow the raw [`ExecutionJournal`] produced by the executor.
    fn journal(&self) -> &ExecutionJournal;
}

#[cfg(feature = "execute")]
impl ExecutionReceiptExt for ExecutionReceipt {
    fn snapshot(&self) -> &CommittedStateSnapshot {
        &self.inner.snapshot
    }

    fn state_after_snapshot(&self) -> &CommittedStateSnapshot {
        &self.inner.state_after
    }

    fn journal(&self) -> &ExecutionJournal {
        &self.inner.journal
    }
}

/// Register a pre-compiled program into the SDK and return its artifact.
pub fn register_compiled(sdk: &Sdk, compiled: CompiledProgram) -> Result<Artifact, SdkError> {
    sdk.register_compiled(compiled)
}

/// Borrow the runtime registries from an environment.
pub fn runtime_registries(environment: &crate::Environment) -> &RuntimeRegistries {
    environment.inner.host_environment.runtime_registries()
}

/// Borrow the host environment from an SDK environment.
pub fn host_environment(environment: &crate::Environment) -> &HostEnvironment {
    &environment.inner.host_environment
}

/// Borrow the compiler catalogs from an SDK environment.
pub fn compiler_catalogs(environment: &crate::Environment) -> &CompilerCatalogs {
    &environment.inner.compiler_catalogs
}

/// Construct a [`HostEnvironment`] from runtime registries and installed schemes.
pub fn build_host_environment(
    runtime_registries: RuntimeRegistries,
    schemes: tabula_runtime::InstalledSchemes,
) -> HostEnvironment {
    HostEnvironment::empty()
        .with_runtime_registries(runtime_registries)
        .with_schemes(schemes)
}

/// Wrap a [`TypeRuntime`] implementor behind a shared reference.
pub fn share_type_runtime(runtime: impl TypeRuntime + 'static) -> Arc<dyn TypeRuntime> {
    Arc::new(runtime)
}

/// Wrap an [`EncodingRuntime`] implementor behind a shared reference.
pub fn share_encoding_runtime(runtime: impl EncodingRuntime + 'static) -> Arc<dyn EncodingRuntime> {
    Arc::new(runtime)
}

/// Prepare the cached native runtime for one artifact.
#[cfg(feature = "execute")]
pub fn prepare_runtime(
    sdk: &Sdk,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::TabulaRuntime>, SdkError> {
    sdk.prepare_runtime(artifact)
}

/// Prepare the cached native prover for one artifact.
#[cfg(feature = "prove")]
pub fn prepare_prover(
    sdk: &Sdk,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::PreparedProver>, SdkError> {
    sdk.prepare_prepared_prover(artifact)
}

/// Prepare the cached native verifier for one artifact.
#[cfg(feature = "verify")]
pub fn prepare_verifier(
    sdk: &Sdk,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::PreparedVerifier>, SdkError> {
    sdk.prepare_prepared_verifier(artifact)
}

/// Construct one SDK execution receipt from raw runtime-owned parts.
#[cfg(feature = "execute")]
pub fn execution_receipt_from_raw_parts(parts: RawExecutionReceiptParts) -> ExecutionReceipt {
    let inner = tabula_runtime::ExecutionReceipt {
        snapshot: parts.snapshot,
        batch: parts.batch,
        context: parts.context,
        journal: parts.journal,
        state_after: parts.state_after_snapshot,
    };
    ExecutionReceipt::from_runtime(
        #[cfg(feature = "prove")]
        parts.program_digest,
        parts.state_before,
        parts.state_after,
        inner,
    )
}
