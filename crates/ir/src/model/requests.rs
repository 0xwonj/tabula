//! Runtime request types: entry calls, batches, and context inputs.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{ContextFieldId, EntryId};

/// A single entry call with serialized parameter values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EntryCall {
    /// The entry to call.
    pub entry_id: EntryId,
    /// Serialized parameter values in declaration order.
    pub params: Vec<PortableValue>,
}

/// An ordered batch of entry calls executed atomically.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct EntryBatch {
    /// Entry calls in execution order.
    pub calls: Vec<EntryCall>,
}

/// The public context values supplied by the caller.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ContextInput {
    /// Mapping from context field identifier to serialized value.
    pub fields: std::collections::BTreeMap<ContextFieldId, PortableValue>,
}
