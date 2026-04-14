use std::sync::Arc;

use tabula_core::{CommittedKey, PortableValue, TypeId};

/// Runtime-only value representation used by executor, witness preparation, and
/// typed capability execution.
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
pub struct CommittedColumnEntry {
    /// Canonical committed key for the entry.
    pub key: CommittedKey,
    /// Typed runtime value for this cell.
    pub value: TypedValue,
    /// Whether the cell is null/absent.
    pub is_null: bool,
}

/// Typed result of resolving one structural property query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedCommittedPropertyQueryResult {
    /// Typed runtime value produced by the query.
    pub value: TypedValue,
    /// Committed key at which the value was found, if any.
    pub key: Option<CommittedKey>,
    /// Whether the result is null.
    pub is_null: bool,
}

/// One logical keyed state cell used by authoring-facing layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedLogicalStateCell {
    /// User-state table identifier.
    pub table: tabula_core::TableId,
    /// Logical key tuple in declaration order.
    pub key: Vec<TypedValue>,
    /// Column identifier.
    pub col: tabula_core::ColId,
    /// Typed runtime value.
    pub value: TypedValue,
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
