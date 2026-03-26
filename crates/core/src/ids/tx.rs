//! Transaction-type identifiers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Unique identifier for a transaction type.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TxTypeId(pub u32);

impl std::fmt::Display for TxTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tx_type:{}", self.0)
    }
}

impl From<u32> for TxTypeId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<TxTypeId> for u32 {
    fn from(id: TxTypeId) -> Self {
        id.0
    }
}
