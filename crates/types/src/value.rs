use std::sync::Arc;

use tabula_core::{PortableValue, RowKey, TypeId};

/// Runtime-only value representation used by executor, witness preparation, and
/// typed precompile execution.
///
/// The payload is canonical type-owned bytes. `TypedValue` is not serialized
/// across artifacts or protocol boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedValue {
    type_id: TypeId,
    payload: Arc<[u8]>,
}

/// Typed committed-column entry used by runtime property resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedColumnEntry {
    /// Row key for the committed cell.
    pub row_key: RowKey,
    /// Typed runtime value for this cell.
    pub value: TypedValue,
    /// Whether the cell is null/absent.
    pub is_null: bool,
}

/// Typed result of resolving one structural property query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPropertyQueryResult {
    /// Typed runtime value produced by the query.
    pub value: TypedValue,
    /// Row key at which the value was found, if any.
    pub key: Option<RowKey>,
    /// Whether the result is null.
    pub is_null: bool,
}

impl TypedValue {
    /// Build a typed runtime value from canonical type-owned payload bytes.
    #[must_use]
    pub fn new(type_id: TypeId, payload: impl Into<Arc<[u8]>>) -> Self {
        Self {
            type_id,
            payload: payload.into(),
        }
    }

    /// Runtime type identifier.
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Canonical runtime payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Convert into portable protocol form.
    #[must_use]
    pub fn into_portable(self) -> PortableValue {
        PortableValue::new(self.type_id, self.payload.to_vec())
    }
}
