//! Poseidon2 hasher: production FieldHasher + byte-level Hasher implementation.

use p3_baby_bear::BabyBear;
use p3_baby_bear::{Poseidon2BabyBear, default_babybear_poseidon2_16};
use p3_symmetric::{
    CryptographicHasher, PaddingFreeSponge, PseudoCompressionFunction, TruncatedPermutation,
};

use tabula_core::traits::{Hasher, ValueCodec};
use tabula_core::{Digest, Value};

use crate::codec::BabyBearCodec;
use crate::field::NativeDigest;
use crate::hasher::FieldHasher;

type Perm = Poseidon2BabyBear<16>;
type Sponge = PaddingFreeSponge<Perm, 16, 8, 8>;
type Compress = TruncatedPermutation<Perm, 2, 8, 16>;

/// Poseidon2 over BabyBear (width=16, rate=8, capacity=8, S-box=x^7).
///
/// Implements both `FieldHasher` (native FE interface for SMT/SSMC) and
/// `Hasher` (byte-level interface for executor compatibility).
#[derive(Clone)]
pub struct PoseidonHasher {
    sponge: Sponge,
    compress: Compress,
}

impl Default for PoseidonHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PoseidonHasher {
    /// Create a new PoseidonHasher with the standard Plonky3 BabyBear configuration.
    pub fn new() -> Self {
        let perm = default_babybear_poseidon2_16();
        Self {
            sponge: PaddingFreeSponge::new(perm.clone()),
            compress: TruncatedPermutation::new(perm),
        }
    }
}

// ── FieldHasher (native FE interface) ───────────────────────────────────────

impl FieldHasher for PoseidonHasher {
    type F = BabyBear;
    type Digest = NativeDigest;

    fn hash(&self, input: &[BabyBear]) -> NativeDigest {
        NativeDigest(self.sponge.hash_slice(input))
    }

    fn compress(&self, left: &NativeDigest, right: &NativeDigest) -> NativeDigest {
        NativeDigest(self.compress.compress([left.0, right.0]))
    }

    fn hash_domain(&self, tag: u32, input: &[BabyBear]) -> NativeDigest {
        let mut prefixed = Vec::with_capacity(1 + input.len());
        prefixed.push(BabyBear::new(tag));
        prefixed.extend_from_slice(input);
        NativeDigest(self.sponge.hash_slice(&prefixed))
    }
}

// ── Hasher (byte-level interface) ───────────────────────────────────────────

impl Hasher for PoseidonHasher {
    fn hash(&self, data: &[u8]) -> Digest {
        // Pack each byte as one BabyBear FE (simple, always canonical).
        let fes: Vec<BabyBear> = data.iter().map(|&b| BabyBear::new(b as u32)).collect();
        let result: NativeDigest = FieldHasher::hash(self, &fes);
        result.to_bytes()
    }

    /// # Panics
    ///
    /// Panics if `left` or `right` contains a non-canonical BabyBear value
    /// (any 4-byte LE chunk >= BabyBear modulus `p = 2013265921`).
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest {
        let left_native =
            NativeDigest::from_bytes(left).expect("hash_pair: left digest non-canonical");
        let right_native =
            NativeDigest::from_bytes(right).expect("hash_pair: right digest non-canonical");
        let result = FieldHasher::compress(self, &left_native, &right_native);
        result.to_bytes()
    }

    /// Override: uses native FE encoding per semantics-spec §1.5.5.
    ///
    /// `Poseidon(0x02 || n || ComEnc(v_0) || ... || ComEnc(v_{n-1}))`
    ///
    /// # Panics
    ///
    /// Panics if any `Value` in `inputs` fails field-element encoding via
    /// `BabyBearCodec::encode` (e.g., `Bytes32` with a non-canonical chunk).
    fn hash_ir(&self, inputs: &[Value]) -> Digest {
        let codec = BabyBearCodec;
        let mut fes = Vec::new();
        fes.push(BabyBear::new(crate::field::DOMAIN_HASH_IR));
        fes.push(BabyBear::new(inputs.len() as u32));
        for v in inputs {
            let encoded = codec.encode(v).expect("hash_ir: encoding failed");
            fes.extend(encoded);
        }
        let result: NativeDigest = FieldHasher::hash(self, &fes);
        result.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::PrimeCharacteristicRing;

    #[test]
    fn poseidon_hash_deterministic() {
        let h = PoseidonHasher::new();
        let input = [BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)];
        assert_eq!(FieldHasher::hash(&h, &input), FieldHasher::hash(&h, &input));
    }

    #[test]
    fn poseidon_hash_distinct() {
        let h = PoseidonHasher::new();
        let a = [BabyBear::new(1), BabyBear::new(2)];
        let b = [BabyBear::new(2), BabyBear::new(1)];
        assert_ne!(FieldHasher::hash(&h, &a), FieldHasher::hash(&h, &b));
    }

    #[test]
    fn poseidon_compress_deterministic() {
        let h = PoseidonHasher::new();
        let left = NativeDigest([BabyBear::new(1); 8]);
        let right = NativeDigest([BabyBear::new(2); 8]);
        assert_eq!(
            FieldHasher::compress(&h, &left, &right),
            FieldHasher::compress(&h, &left, &right)
        );
    }

    #[test]
    fn poseidon_hash_domain_different_tags() {
        let h = PoseidonHasher::new();
        let input = [BabyBear::new(42)];
        let d1 = h.hash_domain(0x00, &input);
        let d2 = h.hash_domain(0x01, &input);
        assert_ne!(d1, d2);
    }

    #[test]
    fn poseidon_byte_hash_deterministic() {
        let h = PoseidonHasher::new();
        let data = b"hello world";
        assert_eq!(Hasher::hash(&h, data), Hasher::hash(&h, data));
    }

    #[test]
    fn poseidon_hash_pair_works() {
        let h = PoseidonHasher::new();
        let left = NativeDigest([BabyBear::ZERO; 8]).to_bytes();
        let right = NativeDigest([BabyBear::ONE; 8]).to_bytes();
        let result = h.hash_pair(&left, &right);
        // Just verify it doesn't panic and returns a valid digest.
        assert_ne!(result, [0u8; 32]);
    }

    #[test]
    fn poseidon_hash_ir_empty() {
        let h = PoseidonHasher::new();
        let result = h.hash_ir(&[]);
        assert_ne!(result, [0u8; 32]);
    }

    #[test]
    fn poseidon_hash_ir_u64() {
        let h = PoseidonHasher::new();
        let result = h.hash_ir(&[Value::U64(42)]);
        assert_ne!(result, [0u8; 32]);
        // Deterministic.
        assert_eq!(result, h.hash_ir(&[Value::U64(42)]));
        // Different value → different hash.
        assert_ne!(result, h.hash_ir(&[Value::U64(43)]));
    }

    #[test]
    fn poseidon_hash_ir_all_types() {
        let h = PoseidonHasher::new();
        let inputs = [
            Value::U64(100),
            Value::I64(-50),
            Value::Bool(true),
            Value::Bytes32([0; 32]),
        ];
        let result = h.hash_ir(&inputs);
        assert_ne!(result, [0u8; 32]);
        assert_eq!(result, h.hash_ir(&inputs));
    }
}
