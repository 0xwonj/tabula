//! Decoded public context values supplied by the caller for one batch.

use std::collections::BTreeMap;

use tabula_ir as ir;

use crate::TypedValue;

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
