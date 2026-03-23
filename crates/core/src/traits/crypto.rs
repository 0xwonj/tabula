//! Cryptographic trait abstractions: hashing, signatures, program membership, batch digests.

use crate::error::TabulaError;
use crate::{Batch, Digest, PortableValue};

/// Domain separation tag for the IR `Hash` instruction.
///
/// Distinct from SSMC (0x00), SMT (0x01), leaf (0x10), tables (0x11), cols (0x12).
pub const DOMAIN_TAG_HASH_IR: u8 = 0x02;

/// Cryptographic hash function abstraction.
///
/// Out-of-circuit: Blake3. In-circuit: Poseidon or other SNARK/STARK-friendly hash.
pub trait Hasher: Send + Sync {
    /// Hash arbitrary data.
    fn hash(&self, data: &[u8]) -> Digest;
    /// Hash two digests together.
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest;
    /// Hash a sequence of byte slices. Default: length-prefix each item to prevent collisions.
    ///
    /// Each item is prefixed with its length as a little-endian u32. This ensures
    /// `hash_many(&["ab", "c"])` differs from `hash_many(&["a", "bc"])`.
    fn hash_many(&self, items: &[&[u8]]) -> Digest {
        let total_len: usize = items.iter().map(|s| 4 + s.len()).sum();
        let mut buf = Vec::with_capacity(total_len);
        for item in items {
            buf.extend_from_slice(&(item.len() as u32).to_le_bytes());
            buf.extend_from_slice(item);
        }
        self.hash(&buf)
    }
    /// Hash IR values using the normative encoding (semantics-spec §1.5.5).
    ///
    /// Encoding: `hash(domain_tag || n_le32 || encode(v_0) || ... || encode(v_{n-1}))`
    /// where `domain_tag` = `DOMAIN_TAG_HASH_IR` (0x02),
    /// `encode(v)` = `type_id_le32 || payload_len_le32 || canonical_payload`.
    fn hash_ir(&self, inputs: &[PortableValue]) -> Digest {
        let mut buf = Vec::new();
        buf.push(DOMAIN_TAG_HASH_IR);
        buf.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
        for v in inputs {
            encode_value_ir(&mut buf, v);
        }
        self.hash(&buf)
    }
}

/// Deterministic type-tagged encoding for IR Hash instruction.
fn encode_value_ir(buf: &mut Vec<u8>, v: &PortableValue) {
    buf.extend_from_slice(&v.type_id().0.to_le_bytes());
    buf.extend_from_slice(&(v.payload().len() as u32).to_le_bytes());
    buf.extend_from_slice(v.payload());
}

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

/// Proves that an item is a member of the committed program (`programRoot`).
///
/// Items are pre-serialized as `&[u8]` (e.g. borsh-encoded `TxTypeDef`).
pub trait MembershipScheme: Send + Sync {
    /// The membership proof type.
    type Proof: Clone + Send + Sync;

    /// Compute `programRoot` from a set of serialized program items.
    fn compute_root(&self, items: &[&[u8]]) -> Result<Digest, TabulaError>;

    /// Generate a membership proof for a specific item.
    fn prove(&self, items: &[&[u8]], index: usize) -> Result<Self::Proof, TabulaError>;

    /// Verify a membership proof.
    fn verify(&self, root: &Digest, item: &[u8], proof: &Self::Proof) -> Result<bool, TabulaError>;
}

/// Computes `batchDigest` from a `Batch`.
pub trait BatchDigester: Send + Sync {
    /// Compute the batch digest.
    fn digest(&self, batch: &Batch) -> Result<Digest, TabulaError>;
}
