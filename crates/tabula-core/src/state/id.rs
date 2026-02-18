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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cellkey_ordering() {
        let a = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        let b = CellKey {
            table: TableId(1),
            col: ColId(1),
            row: RowKey(0),
        };
        let c = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(1),
        };
        let d = CellKey {
            table: TableId(2),
            col: ColId(0),
            row: RowKey(0),
        };

        assert!(a < b, "same table, col 0 < col 1");
        assert!(a < c, "same table+col, row 0 < row 1");
        assert!(c < d, "table 1 < table 2");
        assert!(
            c < b,
            "col 0 row 1 < col 1 row 0: col takes priority over row"
        );
    }

    #[test]
    fn borsh_round_trip_cellkey() {
        let ck = CellKey {
            table: TableId(5),
            col: ColId(3),
            row: RowKey(100),
        };
        let bytes = borsh::to_vec(&ck).unwrap();
        let decoded: CellKey = borsh::from_slice(&bytes).unwrap();
        assert_eq!(ck, decoded);
    }

    #[test]
    fn borsh_round_trip_state_root() {
        let root = StateRoot([0xAB; 32]);
        let bytes = borsh::to_vec(&root).unwrap();
        let decoded: StateRoot = borsh::from_slice(&bytes).unwrap();
        assert_eq!(root, decoded);
    }

    #[test]
    fn display_types() {
        assert_eq!(format!("{}", TableId(5)), "table:5");
        assert_eq!(format!("{}", ColId(3)), "col:3");
        assert_eq!(format!("{}", RowKey(100)), "row:100");
        assert_eq!(format!("{}", TxTypeId(7)), "tx_type:7");
        assert_eq!(
            format!(
                "{}",
                CellKey {
                    table: TableId(1),
                    col: ColId(2),
                    row: RowKey(3)
                }
            ),
            "(1:2:3)"
        );
    }

    #[test]
    fn from_conversions() {
        assert_eq!(TableId::from(5u32), TableId(5));
        assert_eq!(u32::from(TableId(5)), 5);
        assert_eq!(ColId::from(3u16), ColId(3));
        assert_eq!(u16::from(ColId(3)), 3);
        assert_eq!(RowKey::from(100u64), RowKey(100));
        assert_eq!(u64::from(RowKey(100)), 100);
        assert_eq!(TxTypeId::from(7u32), TxTypeId(7));
        assert_eq!(u32::from(TxTypeId(7)), 7);
    }
}
