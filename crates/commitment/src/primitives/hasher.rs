//! Field-element-level hash abstraction for the commitment layer.

use core::fmt::Debug;

#[cfg(test)]
use p3_field::PrimeCharacteristicRing;
#[cfg(test)]
use p3_koala_bear::KoalaBear;

#[cfg(test)]
use super::field::NativeDigest;

/// Field-element-level hash abstraction.
///
/// Distinct from `tabula_core::traits::Hasher` (which is byte-level).
/// Used by SMT, SSMC, and canonical commitment state helpers for native field-element hashing.
pub trait FieldHasher: Clone + Send + Sync {
    /// The field element type.
    type F: Clone + Copy + Default + Eq + Send + Sync;
    /// The digest type (fixed-size output).
    type Digest: Clone + Copy + Default + Eq + Send + Sync + Debug;

    /// Hash a variable-length sequence of field elements.
    fn hash(&self, input: &[Self::F]) -> Self::Digest;
    /// 2-to-1 compression (for Merkle tree internal nodes).
    fn compress(&self, left: &Self::Digest, right: &Self::Digest) -> Self::Digest;
    /// Domain-separated hash (tag prepended before input).
    fn hash_domain(&self, tag: u32, input: &[Self::F]) -> Self::Digest;

    /// The zero/empty digest (identity for empty trees).
    fn zero_digest(&self) -> Self::Digest {
        Self::Digest::default()
    }
}

/// Fast non-cryptographic field hasher for testing.
///
/// Uses a simple position-dependent mixing scheme. NOT cryptographically secure.
/// Useful for testing tree/commitment logic without Poseidon2 overhead.
#[derive(Clone, Debug)]
#[cfg(test)]
pub(crate) struct MockFieldHasher;

#[cfg(test)]
impl FieldHasher for MockFieldHasher {
    type F = KoalaBear;
    type Digest = NativeDigest;

    fn hash(&self, input: &[KoalaBear]) -> NativeDigest {
        let mut state = [KoalaBear::ZERO; 8];
        for (i, &fe) in input.iter().enumerate() {
            let idx = i % 8;
            // Position-dependent mixing: not commutative, deterministic.
            state[idx] = state[idx] * KoalaBear::new(7) + fe + KoalaBear::new(i as u32 + 1);
        }
        NativeDigest(state)
    }

    fn compress(&self, left: &NativeDigest, right: &NativeDigest) -> NativeDigest {
        let mut combined = Vec::with_capacity(16);
        combined.extend_from_slice(&left.0);
        combined.extend_from_slice(&right.0);
        self.hash(&combined)
    }

    fn hash_domain(&self, tag: u32, input: &[KoalaBear]) -> NativeDigest {
        let mut prefixed = Vec::with_capacity(1 + input.len());
        prefixed.push(KoalaBear::new(tag));
        prefixed.extend_from_slice(input);
        self.hash(&prefixed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_hash_deterministic() {
        let h = MockFieldHasher;
        let input = [KoalaBear::new(1), KoalaBear::new(2), KoalaBear::new(3)];
        assert_eq!(h.hash(&input), h.hash(&input));
    }

    #[test]
    fn mock_hash_distinct_inputs() {
        let h = MockFieldHasher;
        let a = [KoalaBear::new(1), KoalaBear::new(2)];
        let b = [KoalaBear::new(2), KoalaBear::new(1)];
        assert_ne!(h.hash(&a), h.hash(&b));
    }

    #[test]
    fn mock_compress_deterministic() {
        let h = MockFieldHasher;
        let left = NativeDigest([KoalaBear::new(1); 8]);
        let right = NativeDigest([KoalaBear::new(2); 8]);
        assert_eq!(h.compress(&left, &right), h.compress(&left, &right));
    }

    #[test]
    fn mock_hash_domain_different_tags() {
        let h = MockFieldHasher;
        let input = [KoalaBear::new(42)];
        let d1 = h.hash_domain(0x00, &input);
        let d2 = h.hash_domain(0x01, &input);
        assert_ne!(d1, d2);
    }

    #[test]
    fn mock_zero_digest_is_default() {
        let h = MockFieldHasher;
        assert_eq!(h.zero_digest(), NativeDigest::default());
    }
}
