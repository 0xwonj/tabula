//! Core trait definitions — pluggable abstractions for crypto-agnosticism.
//!
//! All cryptographic and policy decisions are abstracted behind these traits.
//! The executor and commitment layers are parameterized, not hardcoded.

use crate::error::TabulaError;
use crate::state::Digest;
use crate::tx::{Batch, TxTypeDef};
use crate::types::{CellKey, ColId, RowKey, TableId, Value, ValueType};

// ---------------------------------------------------------------------------
// 1. Hasher
// ---------------------------------------------------------------------------

/// Cryptographic hash function abstraction.
///
/// Out-of-circuit: Blake3. In-circuit: Poseidon or other SNARK/STARK-friendly hash.
pub trait Hasher: Send + Sync {
    /// Hash arbitrary data.
    fn hash(&self, data: &[u8]) -> Digest;
    /// Hash two digests together.
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest;
    /// Hash a sequence of byte slices. Default: concatenate then hash.
    fn hash_many(&self, items: &[&[u8]]) -> Digest {
        let total_len = items.iter().map(|s| s.len()).sum();
        let mut buf = Vec::with_capacity(total_len);
        for item in items {
            buf.extend_from_slice(item);
        }
        self.hash(&buf)
    }
    /// Hash IR values using the normative encoding (semantics-spec §1.5.5).
    ///
    /// Encoding: `hash(domain_tag || n_le32 || encode(v_0) || ... || encode(v_{n-1}))`
    /// where `domain_tag` = `DOMAIN_TAG_HASH_IR` (0x02),
    /// `encode(v)` = `type_tag:u8 || canonical_bytes(v)`.
    ///
    /// Type tags: U64=0, I64=1, Bool=2, Bytes32=3.
    fn hash_ir(&self, inputs: &[Value]) -> Digest {
        let mut buf = Vec::new();
        buf.push(DOMAIN_TAG_HASH_IR);
        buf.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
        for v in inputs {
            encode_value_ir(&mut buf, v);
        }
        self.hash(&buf)
    }
}

/// Domain separation tag for the IR `Hash` instruction.
///
/// Distinct from SSMC (0x00), SMT (0x01), leaf (0x10), tables (0x11), cols (0x12).
pub const DOMAIN_TAG_HASH_IR: u8 = 0x02;

/// Deterministic type-tagged encoding for IR Hash instruction.
fn encode_value_ir(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::U64(n) => {
            buf.push(0);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        Value::I64(n) => {
            buf.push(1);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        Value::Bool(b) => {
            buf.push(2);
            buf.push(u8::from(*b));
        }
        Value::Bytes32(b) => {
            buf.push(3);
            buf.extend_from_slice(b);
        }
    }
}

// ---------------------------------------------------------------------------
// 2. StateSnapshot
// ---------------------------------------------------------------------------

/// Read-only access to the committed state (snapshot).
///
/// The executor uses this to resolve reads that miss the overlay.
pub trait StateSnapshot: Send + Sync {
    /// Read a cell from committed state. Returns `None` if absent.
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError>;
    /// Check whether a table exists.
    fn table_exists(&self, table: TableId) -> bool;
}

// ---------------------------------------------------------------------------
// 3. SigVerifier
// ---------------------------------------------------------------------------

/// Signature verification abstraction.
pub trait SigVerifier: Send + Sync {
    /// Verify a signature. Returns `Ok(())` on success, `Err` on failure.
    fn verify(
        &self,
        sender: &[u8; 32],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), TabulaError>;
}

// ---------------------------------------------------------------------------
// 4. ValueCodec
// ---------------------------------------------------------------------------

/// Encodes/decodes application-level Values to/from field elements.
pub trait ValueCodec: Send + Sync {
    /// The field element representation.
    type FieldRepr: Clone + Send + Sync;

    /// Encode a Value into field elements.
    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError>;

    /// Decode field elements back into a Value.
    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        target_type: ValueType,
    ) -> Result<Value, TabulaError>;

    /// How many field elements a given ValueType requires.
    fn field_elements_per(&self, value_type: ValueType) -> usize;
}

// ---------------------------------------------------------------------------
// 5. NoncePolicy
// ---------------------------------------------------------------------------

/// Replay protection policy abstraction.
pub trait NoncePolicy: Send + Sync {
    /// Validate that a transaction's nonce is acceptable. Returns `Ok(())` on success.
    fn validate(
        &self,
        sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError>;

    /// Compute the next nonce after a successful transaction.
    fn next_nonce(&self, sender: &[u8; 32], current_nonce: u64) -> u64;
}

// ---------------------------------------------------------------------------
// 6. MembershipScheme
// ---------------------------------------------------------------------------

/// Proves that a tx type is a member of the committed program (`programRoot`).
pub trait MembershipScheme: Send + Sync {
    /// The membership proof type.
    type Proof: Clone + Send + Sync;

    /// Compute `programRoot` from a set of tx type definitions.
    fn compute_root(&self, tx_types: &[TxTypeDef]) -> Result<Digest, TabulaError>;

    /// Generate a membership proof for a specific tx type.
    fn prove(&self, tx_types: &[TxTypeDef], index: usize) -> Result<Self::Proof, TabulaError>;

    /// Verify a membership proof.
    fn verify(
        &self,
        root: &Digest,
        tx_type: &TxTypeDef,
        proof: &Self::Proof,
    ) -> Result<bool, TabulaError>;
}

// ---------------------------------------------------------------------------
// 7. BatchDigester
// ---------------------------------------------------------------------------

/// Computes `batchDigest` from a `Batch`.
pub trait BatchDigester: Send + Sync {
    /// Compute the batch digest.
    fn digest(&self, batch: &Batch) -> Result<Digest, TabulaError>;
}

// ---------------------------------------------------------------------------
// 8. StaticTableProvider
// ---------------------------------------------------------------------------

/// Provides read-only access to static (fixed) tables.
///
/// Used by the LOOKUP instruction for range checks, byte decomposition, enum sets, etc.
pub trait StaticTableProvider: Send + Sync {
    /// Lookup a value in a static table.
    fn lookup(&self, table: TableId, key: RowKey, col: ColId) -> Result<Value, TabulaError>;

    /// Check whether a row exists in a static table.
    fn contains(&self, table: TableId, key: RowKey) -> Result<bool, TabulaError>;
}

// ---------------------------------------------------------------------------
// Compile-time trait bound checks
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_hasher_bounds<T: Hasher + Send + Sync>() {}
    fn _assert_state_snapshot_bounds<T: StateSnapshot + Send + Sync>() {}
    fn _assert_sig_verifier_bounds<T: SigVerifier + Send + Sync>() {}
    fn _assert_nonce_policy_bounds<T: NoncePolicy + Send + Sync>() {}
    fn _assert_static_table_provider_bounds<T: StaticTableProvider + Send + Sync>() {}
    fn _assert_batch_digester_bounds<T: BatchDigester + Send + Sync>() {}

    #[test]
    fn test_trait_bounds_compile() {
        // This test simply verifies the trait definitions compile.
        // The actual bound-check functions above are compile-only.
    }
}
