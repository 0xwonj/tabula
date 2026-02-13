//! Mock implementations of all pluggable traits for Phase 1 testing.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::state::Digest;
use tabula_core::traits::*;
use tabula_core::tx::{Batch, TxTypeDef};
use tabula_core::types::*;

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

/// Signature verifier that always returns `true`.
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
    tables: std::collections::BTreeSet<TableId>,
}

impl InMemoryState {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            tables: std::collections::BTreeSet::new(),
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

impl InMemoryState {
    /// Compute the state root over all tables defined in `schemas`.
    ///
    /// Iterates schemas sorted by TableId, gathers column values sorted by
    /// ColId and RowKey, then computes the 3-layer commitment hierarchy:
    /// column → table → state root.
    pub fn compute_state_root(
        &self,
        hasher: &dyn Hasher,
        schemas: &[tabula_core::schema::TableSchema],
        version_tag: &[u8],
    ) -> Result<tabula_core::state::StateRoot, TabulaError> {
        use crate::column::compute_column_commitment;
        use crate::root::compute_state_root;
        use crate::table::{compute_schema_hash, compute_table_commitment};

        let mut sorted_schemas = schemas.to_vec();
        sorted_schemas.sort_by_key(|s| s.id);

        let mut table_commitments = Vec::new();
        for schema in &sorted_schemas {
            let schema_bytes =
                borsh::to_vec(schema).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
            let schema_hash = compute_schema_hash(hasher, &schema_bytes);

            let mut sorted_cols = schema.columns.clone();
            sorted_cols.sort_by_key(|c| c.id);

            let mut col_commitments = Vec::new();
            for col_def in &sorted_cols {
                // Gather all values for this (table, col) sorted by row key
                let mut col_values: Vec<(tabula_core::types::RowKey, Value)> = self
                    .data
                    .iter()
                    .filter(|(k, _)| k.table == schema.id && k.col == col_def.id)
                    .map(|(k, v)| (k.row, v.clone()))
                    .collect();
                col_values.sort_by_key(|(r, _)| *r);

                let values: Vec<Value> = col_values.into_iter().map(|(_, v)| v).collect();
                col_commitments.push(compute_column_commitment(hasher, &values)?);
            }

            table_commitments.push(compute_table_commitment(
                hasher,
                &col_commitments,
                schema.id,
                &schema_hash,
            ));
        }

        Ok(compute_state_root(hasher, &table_commitments, version_tag))
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
// MockColumnCommitment + MockPCS
// ---------------------------------------------------------------------------

/// A mock column commitment: hash of borsh-serialized values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockCommitment(pub Digest);

impl ColumnCommitment for MockCommitment {
    fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

/// A mock PCS: hash-based commitments, empty proofs.
#[derive(Debug, Clone)]
pub struct MockPCS {
    codec: MockValueCodec,
}

impl MockPCS {
    /// Create a new mock PCS.
    pub fn new() -> Self {
        Self {
            codec: MockValueCodec,
        }
    }
}

impl Default for MockPCS {
    fn default() -> Self {
        Self::new()
    }
}

impl PCS for MockPCS {
    type Commitment = MockCommitment;
    type OpenProof = ();
    type UpdateProof = ();
    type Codec = MockValueCodec;

    fn codec(&self) -> &Self::Codec {
        &self.codec
    }

    fn commit(&self, values: &[Value]) -> Result<Self::Commitment, TabulaError> {
        let bytes = borsh::to_vec(values).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
        Ok(MockCommitment(*blake3::hash(&bytes).as_bytes()))
    }

    fn open(
        &self,
        _commitment: &Self::Commitment,
        values: &[Value],
        row: RowKey,
    ) -> Result<(Value, Self::OpenProof), TabulaError> {
        let idx = row.0 as usize;
        let value = values
            .get(idx)
            .cloned()
            .ok_or(TabulaError::RowNotFound(TableId(0), row))?;
        Ok((value, ()))
    }

    fn verify_open(
        &self,
        _commitment: &Self::Commitment,
        _row: RowKey,
        _value: &Value,
        _proof: &Self::OpenProof,
    ) -> Result<bool, TabulaError> {
        Ok(true)
    }

    fn batch_open(
        &self,
        _commitment: &Self::Commitment,
        values: &[Value],
        rows: &[RowKey],
    ) -> Result<(Vec<Value>, Self::OpenProof), TabulaError> {
        let mut result = Vec::new();
        for row in rows {
            let idx = row.0 as usize;
            let value = values
                .get(idx)
                .cloned()
                .ok_or(TabulaError::RowNotFound(TableId(0), *row))?;
            result.push(value);
        }
        Ok((result, ()))
    }

    fn update(
        &self,
        commitment: &Self::Commitment,
        row: RowKey,
        _old_value: &Value,
        new_value: &Value,
    ) -> Result<(Self::Commitment, Self::UpdateProof), TabulaError> {
        let mut data = Vec::new();
        data.extend_from_slice(&commitment.0);
        data.extend_from_slice(&row.0.to_le_bytes());
        data.extend_from_slice(
            &borsh::to_vec(new_value).map_err(|e| TabulaError::EncodingError(e.to_string()))?,
        );
        Ok((MockCommitment(*blake3::hash(&data).as_bytes()), ()))
    }
}

// ---------------------------------------------------------------------------
// FlatHashMembership
// ---------------------------------------------------------------------------

/// Membership scheme: hash all tx types, concatenate, hash result.
/// Proof is the full list (brute-force verification).
#[derive(Debug, Clone)]
pub struct FlatHashMembership;

impl MembershipScheme for FlatHashMembership {
    type Proof = Vec<Digest>;

    fn compute_root(&self, tx_types: &[TxTypeDef]) -> Result<Digest, TabulaError> {
        let hashes: Vec<Digest> = tx_types
            .iter()
            .map(|t| {
                let bytes =
                    borsh::to_vec(t).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
                Ok(*blake3::hash(&bytes).as_bytes())
            })
            .collect::<Result<Vec<_>, TabulaError>>()?;

        let mut all_bytes = Vec::new();
        for h in &hashes {
            all_bytes.extend_from_slice(h);
        }
        Ok(*blake3::hash(&all_bytes).as_bytes())
    }

    fn prove(&self, tx_types: &[TxTypeDef], _index: usize) -> Result<Self::Proof, TabulaError> {
        // Proof is all tx type hashes
        tx_types
            .iter()
            .map(|t| {
                let bytes =
                    borsh::to_vec(t).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
                Ok(*blake3::hash(&bytes).as_bytes())
            })
            .collect()
    }

    fn verify(
        &self,
        root: &Digest,
        tx_type: &TxTypeDef,
        proof: &Self::Proof,
    ) -> Result<bool, TabulaError> {
        let tx_hash = {
            let bytes =
                borsh::to_vec(tx_type).map_err(|e| TabulaError::EncodingError(e.to_string()))?;
            *blake3::hash(&bytes).as_bytes()
        };

        // Check tx is in proof list
        if !proof.contains(&tx_hash) {
            return Ok(false);
        }

        // Recompute root
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
    use tabula_core::tx::{ParamDef, Transaction, TxTypeId};

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
    fn test_mock_value_codec_round_trip() {
        let codec = MockValueCodec;
        let v = Value::U64(42);
        let encoded = codec.encode(&v).unwrap();
        let decoded = codec.decode(&encoded, ValueType::U64).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_mock_pcs_commit_deterministic() {
        let pcs = MockPCS::new();
        let values = vec![Value::U64(1), Value::U64(2)];
        let c1 = pcs.commit(&values).unwrap();
        let c2 = pcs.commit(&values).unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_mock_pcs_open() {
        let pcs = MockPCS::new();
        let values = vec![Value::U64(10), Value::U64(20)];
        let commitment = pcs.commit(&values).unwrap();
        let (val, _proof) = pcs.open(&commitment, &values, RowKey(1)).unwrap();
        assert_eq!(val, Value::U64(20));
    }

    #[test]
    fn test_flat_hash_membership() {
        let scheme = FlatHashMembership;
        let types = vec![TxTypeDef {
            id: TxTypeId(1),
            name: "transfer".into(),
            param_schema: vec![ParamDef {
                name: "amount".into(),
                value_type: ValueType::U64,
            }],
            body: vec![],
        }];

        let root = scheme.compute_root(&types).unwrap();
        let proof = scheme.prove(&types, 0).unwrap();
        assert!(scheme.verify(&root, &types[0], &proof).unwrap());
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

    #[test]
    fn test_mock_pcs_update_changes_commitment() {
        let pcs = MockPCS::new();
        let values = vec![Value::U64(10), Value::U64(20)];
        let c1 = pcs.commit(&values).unwrap();
        let (c2, _) = pcs
            .update(&c1, RowKey(0), &Value::U64(10), &Value::U64(99))
            .unwrap();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_mock_pcs_update_deterministic() {
        let pcs = MockPCS::new();
        let values = vec![Value::U64(10)];
        let c = pcs.commit(&values).unwrap();
        let (u1, _) = pcs
            .update(&c, RowKey(0), &Value::U64(10), &Value::U64(50))
            .unwrap();
        let (u2, _) = pcs
            .update(&c, RowKey(0), &Value::U64(10), &Value::U64(50))
            .unwrap();
        assert_eq!(u1, u2);
    }

    #[test]
    fn test_mock_pcs_update_differs_by_row() {
        let pcs = MockPCS::new();
        let values = vec![Value::U64(10), Value::U64(20)];
        let c = pcs.commit(&values).unwrap();
        let (u1, _) = pcs
            .update(&c, RowKey(0), &Value::U64(10), &Value::U64(50))
            .unwrap();
        let (u2, _) = pcs
            .update(&c, RowKey(1), &Value::U64(20), &Value::U64(50))
            .unwrap();
        assert_ne!(u1, u2);
    }

    #[test]
    fn test_state_root_e2e() {
        use tabula_core::schema::{ColumnDef, TableSchema};

        let schemas = vec![TableSchema {
            id: TableId(1),
            name: "balances".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "balance".into(),
                value_type: ValueType::U64,
            }],
        }];

        let mut state = InMemoryState::new();
        state.set(
            CellKey {
                table: TableId(1),
                col: ColId(0),
                row: RowKey(0),
            },
            Value::U64(100),
        );

        let hasher = MockHasher;
        let root1 = state.compute_state_root(&hasher, &schemas, b"v1").unwrap();
        let root2 = state.compute_state_root(&hasher, &schemas, b"v1").unwrap();
        assert_eq!(root1, root2, "deterministic");

        // Modify state → root changes
        state.set(
            CellKey {
                table: TableId(1),
                col: ColId(0),
                row: RowKey(0),
            },
            Value::U64(200),
        );
        let root3 = state.compute_state_root(&hasher, &schemas, b"v1").unwrap();
        assert_ne!(root1, root3, "state change should produce different root");
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
}
