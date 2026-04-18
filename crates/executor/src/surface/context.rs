//! Execution context types: per-batch host services and public context values.

use tabula_core::traits::Hasher;
use tabula_types::{StateRuntimeView, TypeRuntimeRegistry};

use crate::host::CapabilityExecutor;

/// Host services and type registries threaded through a single batch execution.
pub struct ExecContext<'a> {
    /// Byte-level hash function for use by `Op::Hash`.
    pub hasher: &'a dyn Hasher,
    /// Registry of type codecs for encoding/decoding typed values.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Capability executor, present when native capabilities are available.
    pub capability_executor: Option<&'a dyn CapabilityExecutor>,
    /// Runtime-owned user-state services for committed-key access and property reads.
    pub state_runtime: &'a dyn StateRuntimeView,
}
