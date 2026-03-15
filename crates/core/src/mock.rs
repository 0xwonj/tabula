//! Test utilities requiring blake3.
//!
//! Enabled by the `test-utils` feature flag. For production-grade default
//! implementations (no external deps), see the crate root re-exports.

use crate::error::TabulaError;
use crate::traits::{BatchDigester, Hasher, MembershipScheme, ValueCodec};
use crate::{Batch, Digest, Value, ValueType};

// Re-export default implementations so existing `use tabula_core::mock::*` keeps working.
pub use crate::{InMemoryState, InMemoryStaticTables, NoopSigVerifier, SequentialNonce};

/// Backward-compatible alias. Prefer [`NoopSigVerifier`].
pub type MockSigVerifier = NoopSigVerifier;

// ---------------------------------------------------------------------------
// Blake3Hasher
// ---------------------------------------------------------------------------

/// Hash function backed by blake3.
///
/// Used for non-STARK execution paths (CLI, tests). The STARK proving path
/// uses `PoseidonHasher` from `tabula-commitment`.
#[derive(Debug, Clone, Copy)]
pub struct Blake3Hasher;

/// Backward-compatible alias. Prefer [`Blake3Hasher`].
pub type MockHasher = Blake3Hasher;

impl Hasher for Blake3Hasher {
    fn hash(&self, data: &[u8]) -> Digest {
        *blake3::hash(data).as_bytes()
    }

    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest {
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(left);
        buf[32..].copy_from_slice(right);
        self.hash(&buf)
    }

    fn hash_many(&self, items: &[&[u8]]) -> Digest {
        let mut hasher = blake3::Hasher::new();
        for item in items {
            hasher.update(item);
        }
        *hasher.finalize().as_bytes()
    }
}

// ---------------------------------------------------------------------------
// MockValueCodec
// ---------------------------------------------------------------------------

/// Value codec that uses borsh bytes as the "field representation".
#[derive(Debug, Clone)]
pub struct MockValueCodec;

impl ValueCodec for MockValueCodec {
    type FieldRepr = Vec<u8>;

    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError> {
        let bytes =
            borsh::to_vec(value).map_err(|e| TabulaError::BorshEncodingError(e.to_string()))?;
        Ok(vec![bytes])
    }

    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        _target_type: ValueType,
    ) -> Result<Value, TabulaError> {
        if field_elements.is_empty() {
            return Err(TabulaError::BorshEncodingError(
                "empty field elements".into(),
            ));
        }
        borsh::from_slice(&field_elements[0])
            .map_err(|e| TabulaError::BorshEncodingError(e.to_string()))
    }

    fn field_elements_per(&self, _value_type: ValueType) -> usize {
        1
    }
}

// ---------------------------------------------------------------------------
// FlatHashMembership
// ---------------------------------------------------------------------------

/// Membership scheme: hash all items, concatenate, hash result.
/// Proof is the full list (brute-force verification).
#[derive(Debug, Clone)]
pub struct FlatHashMembership;

impl MembershipScheme for FlatHashMembership {
    type Proof = Vec<Digest>;

    fn compute_root(&self, items: &[&[u8]]) -> Result<Digest, TabulaError> {
        let hashes: Vec<Digest> = items
            .iter()
            .map(|item| *blake3::hash(item).as_bytes())
            .collect();

        let mut all_bytes = Vec::new();
        for h in &hashes {
            all_bytes.extend_from_slice(h);
        }
        Ok(*blake3::hash(&all_bytes).as_bytes())
    }

    fn prove(&self, items: &[&[u8]], _index: usize) -> Result<Self::Proof, TabulaError> {
        Ok(items
            .iter()
            .map(|item| *blake3::hash(item).as_bytes())
            .collect())
    }

    fn verify(&self, root: &Digest, item: &[u8], proof: &Self::Proof) -> Result<bool, TabulaError> {
        let item_hash = *blake3::hash(item).as_bytes();

        if !proof.contains(&item_hash) {
            return Ok(false);
        }

        let mut all_bytes = Vec::new();
        for h in proof {
            all_bytes.extend_from_slice(h);
        }
        let computed = *blake3::hash(&all_bytes).as_bytes();
        Ok(computed == *root)
    }
}

// ---------------------------------------------------------------------------
// SimpleBatchDigester
// ---------------------------------------------------------------------------

/// Batch digester: borsh-serialize, then blake3 hash.
#[derive(Debug, Clone)]
pub struct SimpleBatchDigester;

impl BatchDigester for SimpleBatchDigester {
    fn digest(&self, batch: &Batch) -> Result<Digest, TabulaError> {
        let bytes =
            borsh::to_vec(batch).map_err(|e| TabulaError::BorshEncodingError(e.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Transaction, TxTypeId};

    #[test]
    fn blake3_hasher_deterministic() {
        let h = Blake3Hasher;
        let a = h.hash(b"hello");
        let b = h.hash(b"hello");
        assert_eq!(a, b);
        let c = h.hash(b"world");
        assert_ne!(a, c);
    }

    #[test]
    fn blake3_hasher_pair() {
        let h = Blake3Hasher;
        let a = h.hash(b"left");
        let b = h.hash(b"right");
        let c = h.hash_pair(&a, &b);
        let d = h.hash_pair(&a, &b);
        assert_eq!(c, d);
    }

    #[test]
    fn mock_value_codec_round_trip() {
        let codec = MockValueCodec;
        let v = Value::U64(42);
        let encoded = codec.encode(&v).unwrap();
        let decoded = codec.decode(&encoded, ValueType::U64).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn flat_hash_membership() {
        let scheme = FlatHashMembership;
        let item = b"tx_type_1_serialized";
        let items: Vec<&[u8]> = vec![item.as_slice()];

        let root = scheme.compute_root(&items).unwrap();
        let proof = scheme.prove(&items, 0).unwrap();
        assert!(scheme.verify(&root, item, &proof).unwrap());
    }

    #[test]
    fn simple_batch_digester_deterministic() {
        let digester = SimpleBatchDigester;
        let batch = Batch {
            transactions: vec![Transaction {
                tx_type: TxTypeId(1),
                params: vec![Value::U64(42)],
                sender: [1u8; 32],
                nonce: 0,
                signature: vec![],
            }],
        };
        let d1 = digester.digest(&batch).unwrap();
        let d2 = digester.digest(&batch).unwrap();
        assert_eq!(d1, d2);
    }
}
