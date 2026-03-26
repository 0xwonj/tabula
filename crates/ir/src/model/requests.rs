use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{ContextFieldId, EntryId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EntryCall {
    pub entry_id: EntryId,
    pub params: Vec<PortableValue>,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct EntryBatch {
    pub calls: Vec<EntryCall>,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ContextInput {
    pub fields: std::collections::BTreeMap<ContextFieldId, PortableValue>,
}
