//! Shared test doubles for executor tests.
//!
//! Consolidates common mocks (snapshot, hasher, sig verifier, nonce policy,
//! static tables) used across interpreter, batch, overlay, consistency, and
//! proptest modules.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, NoncePolicy, SigVerifier, StateSnapshot, StaticTableProvider};
use tabula_core::tx::Transaction;
use tabula_core::types::*;

// ---------------------------------------------------------------------------
// StateSnapshot impls
// ---------------------------------------------------------------------------

/// Simple BTreeMap-backed snapshot for tests.
pub(crate) struct TestSnapshot(pub BTreeMap<CellKey, Value>);

impl StateSnapshot for TestSnapshot {
    fn read(&self, key: &CellKey) -> Result<Value, TabulaError> {
        Ok(self.0.get(key).cloned().unwrap_or(Value::Null))
    }

    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

/// Snapshot that tracks how many times `read()` is called.
pub(crate) struct CountingSnapshot {
    pub data: BTreeMap<CellKey, Value>,
    pub call_count: AtomicU32,
}

impl CountingSnapshot {
    pub fn new(data: BTreeMap<CellKey, Value>) -> Self {
        Self {
            data,
            call_count: AtomicU32::new(0),
        }
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl StateSnapshot for CountingSnapshot {
    fn read(&self, key: &CellKey) -> Result<Value, TabulaError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(self.data.get(key).cloned().unwrap_or(Value::Null))
    }

    fn table_exists(&self, _: TableId) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Hasher
// ---------------------------------------------------------------------------

/// XOR-based hasher for deterministic testing.
pub(crate) struct XorHasher;

impl Hasher for XorHasher {
    fn hash(&self, data: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, b) in data.iter().enumerate() {
            out[i % 32] ^= b;
        }
        out
    }

    fn hash_pair(&self, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut combined = Vec::new();
        combined.extend_from_slice(left);
        combined.extend_from_slice(right);
        self.hash(&combined)
    }
}

// ---------------------------------------------------------------------------
// SigVerifier impls
// ---------------------------------------------------------------------------

/// Always-valid signature verifier.
pub(crate) struct AlwaysValidSig;

impl SigVerifier for AlwaysValidSig {
    fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8]) -> Result<(), TabulaError> {
        Ok(())
    }
}

/// Always-invalid signature verifier.
pub(crate) struct AlwaysInvalidSig;

impl SigVerifier for AlwaysInvalidSig {
    fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8]) -> Result<(), TabulaError> {
        Err(TabulaError::SignatureInvalid)
    }
}

// ---------------------------------------------------------------------------
// NoncePolicy
// ---------------------------------------------------------------------------

/// Sequential nonce: `tx_nonce == current`, next = `current + 1`.
pub(crate) struct SeqNonce;

impl NoncePolicy for SeqNonce {
    fn validate(&self, _: &[u8; 32], tx_nonce: u64, current: u64) -> Result<(), TabulaError> {
        if tx_nonce == current {
            Ok(())
        } else {
            Err(TabulaError::InvalidNonce {
                sender: [0u8; 32],
                expected: current,
                actual: tx_nonce,
            })
        }
    }

    fn next_nonce(&self, _: &[u8; 32], current: u64) -> u64 {
        current + 1
    }
}

// ---------------------------------------------------------------------------
// StaticTableProvider impls
// ---------------------------------------------------------------------------

/// Static table that returns `Value::U64(row_key)` for any lookup.
pub(crate) struct TestStaticTables;

impl StaticTableProvider for TestStaticTables {
    fn lookup(&self, _: TableId, key: RowKey, _: ColId) -> Result<Value, TabulaError> {
        Ok(Value::U64(key.0))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

/// Static table that always returns `TableNotFound`.
pub(crate) struct EmptyStaticTables;

impl StaticTableProvider for EmptyStaticTables {
    fn lookup(&self, t: TableId, _: RowKey, _: ColId) -> Result<Value, TabulaError> {
        Err(TabulaError::TableNotFound(t))
    }

    fn contains(&self, _: TableId, _: RowKey) -> Result<bool, TabulaError> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Helper constructors
// ---------------------------------------------------------------------------

/// Shorthand for building a `CellKey`.
pub(crate) fn cell(t: u32, r: u64, c: u16) -> CellKey {
    CellKey {
        table: TableId(t),
        col: ColId(c),
        row: RowKey(r),
    }
}

/// Build a `Transaction` with an empty signature.
pub(crate) fn make_tx(
    tx_type: u32,
    params: Vec<Value>,
    sender: [u8; 32],
    nonce: u64,
) -> Transaction {
    Transaction {
        tx_type: tabula_core::tx::TxTypeId(tx_type),
        params,
        sender,
        nonce,
        signature: vec![],
    }
}

/// Build a `BatchEnv` using the standard test doubles.
pub(crate) fn test_env() -> crate::batch::BatchEnv<'static> {
    crate::batch::BatchEnv {
        hasher: &XorHasher,
        sig_verifier: &AlwaysValidSig,
        nonce_policy: &SeqNonce,
        static_tables: &TestStaticTables,
    }
}
