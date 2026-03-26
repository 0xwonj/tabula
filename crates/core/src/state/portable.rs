//! Canonical portable value carrier for public and serialized boundaries.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::ids::TypeId;

/// Canonical self-describing value used for serialized/public boundaries.
///
/// The payload is canonical type-owned bytes. Portable values do not encode
/// nullability; absence remains `Option<PortableValue>`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct PortableValue {
    type_id: TypeId,
    payload: Vec<u8>,
}

impl PortableValue {
    /// Build one portable value from a sealed semantic type id and canonical
    /// payload bytes.
    #[must_use]
    pub fn new(type_id: TypeId, payload: Vec<u8>) -> Self {
        Self { type_id, payload }
    }

    /// Borrow the sealed semantic type id.
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Borrow the canonical payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consume the value and return the canonical payload bytes.
    #[must_use]
    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}
