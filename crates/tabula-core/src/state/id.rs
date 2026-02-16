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
pub struct CellKey {
    /// The table containing this cell.
    pub table: TableId,
    /// The column within the table.
    pub col: ColId,
    /// The row within the table.
    pub row: RowKey,
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
    serde::Serialize,
    serde::Deserialize,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub struct TxTypeId(pub u32);

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
}
