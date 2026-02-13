//! Column commitment: hash of borsh-serialized column values.

use tabula_core::error::TabulaError;
use tabula_core::state::ColumnCommitmentId;
use tabula_core::traits::Hasher;
use tabula_core::types::Value;

/// Compute a column commitment by borsh-encoding all values and hashing.
///
/// `colCom = H(borsh(values))`
pub fn compute_column_commitment(
    hasher: &dyn Hasher,
    values: &[Value],
) -> Result<ColumnCommitmentId, TabulaError> {
    let bytes = borsh::to_vec(values).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
    Ok(ColumnCommitmentId(hasher.hash(&bytes)))
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
        let vals = vec![Value::U64(1), Value::U64(2), Value::U64(3)];
        let c1 = compute_column_commitment(&h, &vals).unwrap();
        let c2 = compute_column_commitment(&h, &vals).unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_different_inputs_differ() {
        let h = test_hasher();
        let c1 = compute_column_commitment(&h, &[Value::U64(1)]).unwrap();
        let c2 = compute_column_commitment(&h, &[Value::U64(2)]).unwrap();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_empty_input() {
        let h = test_hasher();
        let c = compute_column_commitment(&h, &[]).unwrap();
        // Should succeed and produce some digest
        assert_ne!(c.0, [0u8; 32]);
    }
}
