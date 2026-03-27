//! Execution context types: per-batch host services and public context values.

use std::collections::BTreeMap;

use tabula_core::traits::Hasher;
use tabula_ir as ir;
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::host::{CapabilityExecutor, PropertyReadExecutor};

/// Host services and type registries threaded through a single batch execution.
pub struct ExecContext<'a> {
    /// Byte-level hash function for use by `Op::Hash`.
    pub hasher: &'a dyn Hasher,
    /// Registry of type codecs for encoding/decoding typed values.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Capability executor, present when native capabilities are available.
    pub capability_executor: Option<&'a dyn CapabilityExecutor>,
    /// Property read executor, present when structural state queries are available.
    pub property_reads: Option<&'a dyn PropertyReadExecutor>,
}

/// The decoded public context values supplied by the caller for one batch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextValues {
    /// Mapping from context field ID to its decoded value.
    pub fields: BTreeMap<ir::ContextFieldId, TypedValue>,
}

impl ContextValues {
    /// Create an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a context field value.
    pub fn insert(&mut self, id: ir::ContextFieldId, value: TypedValue) {
        self.fields.insert(id, value);
    }
}
