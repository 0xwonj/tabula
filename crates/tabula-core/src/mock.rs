//! Mock implementations of pluggable traits for Phase 1 testing.
//!
//! Enabled by the `mock` feature flag. These implement the traits defined
//! in [`crate::traits`] using blake3 for hashing and in-memory collections
//! for state.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::error::TabulaError;
use crate::traits::*;
use crate::{Batch, CellKey, ColId, Digest, RowKey, TableId, Value, ValueType};

// ---------------------------------------------------------------------------
// MockHasher (blake3)
// ---------------------------------------------------------------------------

/// Hash function backed by blake3.
#[derive(Debug, Clone)]
pub struct MockHasher;

impl Hasher for MockHasher {
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
// MockSigVerifier (always true)
// ---------------------------------------------------------------------------

/// Signature verifier that always returns `Ok(())`.
#[derive(Debug, Clone)]
pub struct MockSigVerifier;

impl SigVerifier for MockSigVerifier {
    fn verify(
        &self,
        _sender: &[u8; 32],
        _message: &[u8],
        _signature: &[u8],
    ) -> Result<(), TabulaError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SequentialNonce
// ---------------------------------------------------------------------------

/// Nonce policy: `tx_nonce == current_nonce`, next = `current + 1`.
#[derive(Debug, Clone)]
pub struct SequentialNonce;

impl NoncePolicy for SequentialNonce {
    fn validate(
        &self,
        _sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError> {
        if tx_nonce == current_nonce {
            Ok(())
        } else {
            Err(TabulaError::InvalidNonce {
                sender: [0u8; 32],
                expected: current_nonce,
                actual: tx_nonce,
            })
        }
    }

    fn next_nonce(&self, _sender: &[u8; 32], current_nonce: u64) -> u64 {
        current_nonce + 1
    }
}

// ---------------------------------------------------------------------------
// InMemoryStaticTables
// ---------------------------------------------------------------------------

/// BTreeMap-backed static table provider.
#[derive(Debug, Clone)]
pub struct InMemoryStaticTables {
    data: BTreeMap<(TableId, RowKey, ColId), Value>,
}

impl InMemoryStaticTables {
    /// Create a new empty static table provider.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    /// Insert a value into the static tables.
    pub fn insert(&mut self, table: TableId, key: RowKey, col: ColId, value: Value) {
        self.data.insert((table, key, col), value);
    }
}

impl Default for InMemoryStaticTables {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticTableProvider for InMemoryStaticTables {
    fn lookup(&self, table: TableId, key: RowKey, col: ColId) -> Result<Value, TabulaError> {
        self.data
            .get(&(table, key, col))
            .cloned()
            .ok_or(TabulaError::CellNotFound(CellKey {
                table,
                col,
                row: key,
            }))
    }

    fn contains(&self, table: TableId, key: RowKey) -> Result<bool, TabulaError> {
        Ok(self.data.keys().any(|(t, k, _)| *t == table && *k == key))
    }
}

// ---------------------------------------------------------------------------
// InMemoryState (StateSnapshot)
// ---------------------------------------------------------------------------

/// BTreeMap-backed state snapshot.
#[derive(Debug, Clone)]
pub struct InMemoryState {
    data: BTreeMap<CellKey, Value>,
    tables: BTreeSet<TableId>,
}

impl InMemoryState {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            tables: BTreeSet::new(),
        }
    }

    /// Set a cell value.
    pub fn set(&mut self, key: CellKey, value: Value) {
        self.tables.insert(key.table);
        self.data.insert(key, value);
    }
}

impl Default for InMemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl StateSnapshot for InMemoryState {
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        Ok(self.data.get(key).cloned())
    }

    fn table_exists(&self, table: TableId) -> bool {
        self.tables.contains(&table)
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
        let bytes = borsh::to_vec(value).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
        Ok(vec![bytes])
    }

    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        _target_type: ValueType,
    ) -> Result<Value, TabulaError> {
        if field_elements.is_empty() {
            return Err(TabulaError::EncodingError("empty field elements".into()));
        }
        borsh::from_slice(&field_elements[0]).map_err(|e| TabulaError::EncodingError(e.to_string()))
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
        let bytes = borsh::to_vec(batch).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Transaction, TxTypeId};

    #[test]
    fn test_mock_hasher_deterministic() {
        let h = MockHasher;
        let a = h.hash(b"hello");
        let b = h.hash(b"hello");
        assert_eq!(a, b);
        let c = h.hash(b"world");
        assert_ne!(a, c);
    }

    #[test]
    fn test_mock_hasher_pair() {
        let h = MockHasher;
        let a = h.hash(b"left");
        let b = h.hash(b"right");
        let c = h.hash_pair(&a, &b);
        let d = h.hash_pair(&a, &b);
        assert_eq!(c, d);
    }

    #[test]
    fn test_sequential_nonce() {
        let n = SequentialNonce;
        let sender = [0u8; 32];
        assert!(n.validate(&sender, 0, 0).is_ok());
        assert!(n.validate(&sender, 1, 0).is_err());
        assert_eq!(n.next_nonce(&sender, 0), 1);
    }

    #[test]
    fn test_in_memory_static_tables() {
        let mut st = InMemoryStaticTables::new();
        st.insert(TableId(1), RowKey(0), ColId(0), Value::U64(42));

        assert_eq!(
            st.lookup(TableId(1), RowKey(0), ColId(0)).unwrap(),
            Value::U64(42)
        );
        assert!(st.contains(TableId(1), RowKey(0)).unwrap());
        assert!(!st.contains(TableId(1), RowKey(999)).unwrap());
        assert!(st.lookup(TableId(1), RowKey(999), ColId(0)).is_err());
    }

    #[test]
    fn test_in_memory_state() {
        let mut state = InMemoryState::new();
        let k = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        state.set(k, Value::U64(100));

        assert_eq!(state.read(&k).unwrap(), Some(Value::U64(100)));
        assert!(state.table_exists(TableId(1)));
        assert!(!state.table_exists(TableId(99)));
    }

    #[test]
    fn test_in_memory_state_none_for_missing() {
        let state = InMemoryState::new();
        let k = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        assert_eq!(state.read(&k).unwrap(), None);
    }

    #[test]
    fn test_mock_value_codec_round_trip() {
        let codec = MockValueCodec;
        let v = Value::U64(42);
        let encoded = codec.encode(&v).unwrap();
        let decoded = codec.decode(&encoded, ValueType::U64).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_flat_hash_membership() {
        let scheme = FlatHashMembership;
        let item = b"tx_type_1_serialized";
        let items: Vec<&[u8]> = vec![item.as_slice()];

        let root = scheme.compute_root(&items).unwrap();
        let proof = scheme.prove(&items, 0).unwrap();
        assert!(scheme.verify(&root, item, &proof).unwrap());
    }

    #[test]
    fn test_simple_batch_digester_deterministic() {
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
