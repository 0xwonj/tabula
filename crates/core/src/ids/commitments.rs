//! Commitment wrappers and digest aliases.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// A 256-bit cryptographic digest.
pub type Digest = [u8; 32];

/// The global state root, derived from all table commitments.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct StateRoot(pub Digest);

/// Identifier for a table-level commitment.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct TableCommitmentId(pub Digest);

/// Identifier for a column-level commitment.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ColumnCommitmentId(pub Digest);
