//! Identifiers and addresses for state, cells, and commitments.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

// ── Cell addressing ─────────────────────────────────────────────────────

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

/// Identifies a column commitment scheme in portable artifacts.
///
/// This is the protocol-facing identifier that links compiler-selected
/// column proof plans to runtime-installed scheme implementations.
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
pub struct SchemeId(pub u16);

impl SchemeId {
    /// Built-in sorted-state Merkle commitment scheme.
    pub const SSMC: Self = Self(0);
    /// Built-in sparse Merkle tree scheme.
    pub const SMT: Self = Self(1);

    /// Return the raw protocol identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Identifies the verifier-relevant commitment layout/backend for a column.
///
/// Unlike [`SchemeId`], this is not the public SDK/profile identity. It seals
/// the actual column-state representation expected by witness generation and
/// proof chips.
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
pub struct ColumnLayoutKind(pub u16);

impl ColumnLayoutKind {
    /// Built-in sorted-state Merkle commitment layout.
    pub const SSMC_V1: Self = Self(0);
    /// Built-in sparse Merkle tree layout.
    pub const SMT_V1: Self = Self(1);

    /// Return the raw layout identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Identifies a root-proof compatibility profile in portable artifacts.
///
/// Column commitment schemes bind to one root profile so runtime and verifier
/// can fail closed when a artifact and installed root proof disagree.
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
pub struct RootProfileId(pub u16);

impl RootProfileId {
    /// Two-level SMT root proof profile used by Tabula v1.
    pub const SMT_V1: Self = Self(0);

    /// Return the raw protocol identifier.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

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

// ── Transaction type identifiers ────────────────────────────────────────

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

// ── Display impls ──────────────────────────────────────────────────────

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

impl std::fmt::Display for SchemeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scheme:{}", self.0)
    }
}

impl std::fmt::Display for ColumnLayoutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "column_layout:{}", self.0)
    }
}

impl std::fmt::Display for RootProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "root_profile:{}", self.0)
    }
}

impl std::fmt::Display for RowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "row:{}", self.0)
    }
}

impl std::fmt::Display for TxTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tx_type:{}", self.0)
    }
}

impl std::fmt::Display for CellKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}:{}:{})", self.table.0, self.col.0, self.row.0)
    }
}

// ── From conversions ───────────────────────────────────────────────────

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

impl From<u16> for SchemeId {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<SchemeId> for u16 {
    fn from(id: SchemeId) -> Self {
        id.0
    }
}

impl From<u16> for RootProfileId {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl From<RootProfileId> for u16 {
    fn from(id: RootProfileId) -> Self {
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

// ── Commitment identifiers ──────────────────────────────────────────────

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
