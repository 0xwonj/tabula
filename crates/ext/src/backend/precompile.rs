use std::sync::Arc;

pub use tabula_contract::ProgramBinding;
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::backend::AnyRap;
use crate::error::ExtResult;
use crate::precompile::{PrecompileDescriptor, PrecompileId};

/// Bus-visible header for one precompile call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecompileCallHeader {
    /// Zero-based transaction index.
    pub tx_index: u32,
    /// Zero-based instruction index in the tx body.
    pub instruction_index: u32,
    /// Precompile identifier.
    pub precompile_id: u16,
    /// Number of input values.
    pub input_count: u32,
    /// Number of output values.
    pub output_count: u32,
    /// Canonical transcript digest, encoded as the first 8 Poseidon outputs.
    pub event_digest: [u32; 8],
}

/// Resolved verifier-visible contract for one installed precompile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPrecompile {
    /// Sealed descriptor expected by runtime and verifier.
    pub descriptor: PrecompileDescriptor,
}

/// One resolved precompile call bound to a canonical transcript header.
#[cfg(feature = "prove")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPrecompileCall {
    /// Structured execution event.
    pub event: tabula_core::PrecompileEvent,
    /// Canonical transcript header for this call.
    pub header: PrecompileCallHeader,
}

/// Domain-specific execution-tier proof system for one precompile family.
pub trait PrecompileProofSystem: Send + Sync {
    /// Human-readable precompile name.
    fn name(&self) -> &str;

    /// Sealed descriptor for this precompile proof system.
    fn descriptor(&self) -> PrecompileDescriptor;

    /// AIR implementations for proving and verification.
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Dynamic chips for trace generation.
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Optional dependent bus consumers.
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![]
    }
}

/// Backend-neutral precompile proof preparation context.
#[cfg(feature = "prove")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrecompileProofContext {
    /// Bound descriptor for the precompile family being proven.
    pub descriptor: PrecompileDescriptor,
    /// Structured calls for this descriptor, in execution order.
    pub calls: Vec<ResolvedPrecompileCall>,
    /// Artifact/program binding metadata expected by verifier and statement builder.
    pub binding: ProgramBinding,
}

/// Prepared backend-aware proof product for one precompile family.
#[cfg(feature = "prove")]
pub struct PreparedPrecompileProof {
    /// Backend witness store for this precompile proof contribution.
    pub store: crate::backend::WitnessStore,
}

#[cfg(feature = "prove")]
impl std::fmt::Debug for PreparedPrecompileProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedPrecompileProof")
            .finish_non_exhaustive()
    }
}

/// Per-precompile proof preparer.
#[cfg(feature = "prove")]
pub trait PrecompileProofPreparer: Send + Sync {
    /// Human-readable precompile name.
    fn name(&self) -> &str;

    /// Portable precompile identifier.
    fn precompile_id(&self) -> PrecompileId;

    /// Prepare witness data for this precompile family.
    fn prepare_precompile(
        &self,
        context: PrecompileProofContext,
    ) -> ExtResult<PreparedPrecompileProof>;
}

/// Proof factory for one precompile capability family.
pub trait PrecompileProofFactory: Send + Sync {
    /// Sealed descriptor for this precompile family.
    fn descriptor(&self) -> PrecompileDescriptor;

    /// Portable precompile identifier implemented by this factory.
    fn precompile_id(&self) -> PrecompileId {
        self.descriptor().precompile_id
    }

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Build the execution-tier proof system for this precompile family.
    #[cfg(feature = "verify")]
    fn build_system(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> ExtResult<Arc<dyn PrecompileProofSystem>>;

    /// Build the proof preparer for this precompile family.
    #[cfg(feature = "prove")]
    fn build_preparer(
        &self,
        resolved: &ResolvedPrecompile,
    ) -> ExtResult<Arc<dyn PrecompileProofPreparer>>;
}
