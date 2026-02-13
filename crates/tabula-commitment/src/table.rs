//! Table commitment: hash of column commitments, table ID, and schema hash.

use tabula_core::state::{ColumnCommitmentId, Digest, TableCommitmentId};
use tabula_core::traits::Hasher;
use tabula_core::types::TableId;

/// Compute a table commitment from its column commitments.
///
/// `tableCom = H(colCom_1 || ... || colCom_k || tableId_bytes || schemaHash)`
pub fn compute_table_commitment(
    hasher: &dyn Hasher,
    col_commitments: &[ColumnCommitmentId],
    table_id: TableId,
    schema_hash: &Digest,
) -> TableCommitmentId {
    let mut buf = Vec::new();
    for cc in col_commitments {
        buf.extend_from_slice(&cc.0);
    }
    buf.extend_from_slice(&table_id.0.to_le_bytes());
    buf.extend_from_slice(schema_hash);
    TableCommitmentId(hasher.hash(&buf))
}

/// Compute a deterministic hash of a schema's serialized bytes.
pub fn compute_schema_hash(hasher: &dyn Hasher, schema_bytes: &[u8]) -> Digest {
    hasher.hash(schema_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hasher() -> impl Hasher {
        crate::mock::MockHasher
    }

    #[test]
    fn test_deterministic() {
        let h = test_hasher();
        let cols = vec![ColumnCommitmentId([1u8; 32]), ColumnCommitmentId([2u8; 32])];
        let sh = [0xABu8; 32];
        let c1 = compute_table_commitment(&h, &cols, TableId(1), &sh);
        let c2 = compute_table_commitment(&h, &cols, TableId(1), &sh);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_different_table_id_differs() {
        let h = test_hasher();
        let cols = vec![ColumnCommitmentId([1u8; 32])];
        let sh = [0u8; 32];
        let c1 = compute_table_commitment(&h, &cols, TableId(1), &sh);
        let c2 = compute_table_commitment(&h, &cols, TableId(2), &sh);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_schema_hash_deterministic() {
        let h = test_hasher();
        let s1 = compute_schema_hash(&h, b"schema_v1");
        let s2 = compute_schema_hash(&h, b"schema_v1");
        assert_eq!(s1, s2);
        let s3 = compute_schema_hash(&h, b"schema_v2");
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_empty_columns() {
        let h = test_hasher();
        let sh = [0u8; 32];
        let c = compute_table_commitment(&h, &[], TableId(1), &sh);
        assert_ne!(c.0, [0u8; 32]);
    }
}
