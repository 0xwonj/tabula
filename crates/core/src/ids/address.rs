//! Cell-addressing identifiers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Identifies a table in the state.
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
pub struct TableId(pub u32);

/// Identifies a column within a table.
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
pub struct ColId(pub u16);

/// Row key. Dense integer keys for kernel v1.0.
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
pub struct RowKey(pub u64);

/// Canonical committed key for one user-state table row.
#[derive(
    Debug,
    Clone,
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
pub struct CommittedKey(pub Vec<u8>);

/// A fully-qualified cell address.
///
/// **Canonical ordering: `(table, col, row)`** — this is protocol-critical.
/// The proof spec, SSMC trace layout, and GlobalSortedMem all depend on this
/// ordering for correctness. Do not change the `Ord` implementation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CellKey {
    /// The table containing this cell.
    pub table: TableId,
    /// The column within the table.
    pub col: ColId,
    /// The row within the table.
    pub row: RowKey,
}

/// A fully-qualified committed user-state cell address.
///
/// This is the canonical protocol-visible address form for user-state
/// execution, snapshots, and proof preparation.
#[derive(
    Debug,
    Clone,
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
pub struct CommittedCellKey {
    /// The table containing this cell.
    pub table: TableId,
    /// The column within the table.
    pub col: ColId,
    /// The committed key within the table.
    pub key: CommittedKey,
}

impl PartialOrd for CellKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CellKey {
    /// Canonical ordering: table → col → row.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.table
            .cmp(&other.table)
            .then(self.col.cmp(&other.col))
            .then(self.row.cmp(&other.row))
    }
}

impl std::fmt::Display for TableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "table:{}", self.0)
    }
}

impl std::fmt::Display for ColId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "col:{}", self.0)
    }
}

impl std::fmt::Display for RowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "row:{}", self.0)
    }
}

impl std::fmt::Display for CommittedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "key:{}b", self.0.len())
    }
}

impl std::fmt::Display for CellKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}:{}:{})", self.table.0, self.col.0, self.row.0)
    }
}

impl std::fmt::Display for CommittedCellKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}:{}:{})", self.table.0, self.col.0, self.key)
    }
}

impl From<u32> for TableId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<TableId> for u32 {
    fn from(id: TableId) -> Self {
        id.0
    }
}

impl From<u16> for ColId {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<ColId> for u16 {
    fn from(id: ColId) -> Self {
        id.0
    }
}

impl From<u64> for RowKey {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl From<RowKey> for u64 {
    fn from(key: RowKey) -> Self {
        key.0
    }
}

impl From<Vec<u8>> for CommittedKey {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<CommittedKey> for Vec<u8> {
    fn from(value: CommittedKey) -> Self {
        value.0
    }
}
