//! Shared query vocabulary used across IR, artifacts, and profile contracts.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Kind of structural property query on committed column state.
///
/// Closed enum — apps extend support via custom column schemes, not custom
/// query variants.
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
pub enum PropertyQueryKind {
    /// Find the row with the minimum value.
    Minimum,
    /// Find the row with the maximum value.
    Maximum,
    /// Find the row immediately after a given key.
    Successor,
    /// Find the row immediately before a given key.
    Predecessor,
    /// Prove no keys exist in a given range.
    NonExistenceRange,
    /// Compute an aggregate over column values.
    Aggregate,
}

impl PropertyQueryKind {
    /// Canonical proof-time ordinal used in execution/property traces.
    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Minimum => 0,
            Self::Maximum => 1,
            Self::Successor => 2,
            Self::Predecessor => 3,
            Self::NonExistenceRange => 4,
            Self::Aggregate => 5,
        }
    }
}
