//! Global state root computation.

use tabula_core::state::{StateRoot, TableCommitmentId};
use tabula_core::traits::Hasher;

/// Compute the global state root from table commitments and a version tag.
///
/// `stateRoot = H(tableCom_1 || ... || tableCom_m || versionTag)`
pub fn compute_state_root(
    hasher: &dyn Hasher,
    table_commitments: &[TableCommitmentId],
    version_tag: &[u8],
) -> StateRoot {
    let mut buf = Vec::new();
    for tc in table_commitments {
        buf.extend_from_slice(&tc.0);
    }
    buf.extend_from_slice(version_tag);
    StateRoot(hasher.hash(&buf))
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
        let tcs = vec![TableCommitmentId([1u8; 32]), TableCommitmentId([2u8; 32])];
        let r1 = compute_state_root(&h, &tcs, b"v1");
        let r2 = compute_state_root(&h, &tcs, b"v1");
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_different_inputs_differ() {
        let h = test_hasher();
        let tcs = vec![TableCommitmentId([1u8; 32])];
        let r1 = compute_state_root(&h, &tcs, b"v1");
        let r2 = compute_state_root(&h, &tcs, b"v2");
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_empty_input() {
        let h = test_hasher();
        let r = compute_state_root(&h, &[], b"v1");
        assert_ne!(r.0, [0u8; 32]);
    }
}
