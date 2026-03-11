//! BLAKE3 Merkle hash wrappers for the Plonky3 PCS.
//!
//! Wraps BLAKE3 to operate in BabyBear field-element space, producing
//! `[BabyBear; 8]` digests. This keeps compatibility with the
//! [`DuplexChallenger`] which only observes `Hash<F, F, N>` commitments.
//!
//! BLAKE3 replaces Poseidon2 for Merkle tree hashing only — the in-circuit
//! hash (PoseidonChip) and Fiat-Shamir challenger remain Poseidon2.

use p3_baby_bear::BabyBear;
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_symmetric::{CryptographicHasher, PseudoCompressionFunction};

/// BLAKE3-based leaf hasher producing field-element digests.
///
/// Hashes a sequence of `BabyBear` values by converting to bytes,
/// running BLAKE3, and mapping the 32-byte output to `[BabyBear; 8]`.
#[derive(Clone)]
pub struct Blake3FieldHasher;

/// BLAKE3-based Merkle inner-node compressor in field-element space.
///
/// Compresses two `[BabyBear; 8]` digests into one by converting to bytes,
/// running BLAKE3, and mapping back.
#[derive(Clone)]
pub struct Blake3FieldCompressor;

/// Convert 32 bytes to 8 BabyBear field elements.
///
/// Each 4-byte chunk is read as a little-endian `u32` and reduced mod p.
/// Since `p = 2^31 - 2^27 + 1`, values in `[0, 2^32)` are reduced by
/// at most one subtraction, preserving near-uniform distribution.
fn bytes_to_field_8(bytes: &[u8; 32]) -> [BabyBear; 8] {
    let mut out = [BabyBear::ZERO; 8];
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        let val = u32::from_le_bytes(chunk.try_into().expect("chunk is 4 bytes"));
        out[i] = BabyBear::from_u64(val as u64);
    }
    out
}

/// Convert 8 BabyBear field elements to 32 bytes.
fn field_8_to_bytes(fields: &[BabyBear; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, f) in fields.iter().enumerate() {
        let val = f.as_canonical_u32();
        out[4 * i..4 * i + 4].copy_from_slice(&val.to_le_bytes());
    }
    out
}

// ── Leaf Hasher ──────────────────────────────────────────────────────────

impl CryptographicHasher<BabyBear, [BabyBear; 8]> for Blake3FieldHasher {
    fn hash_iter<I: IntoIterator<Item = BabyBear>>(&self, input: I) -> [BabyBear; 8] {
        let mut hasher = blake3::Hasher::new();
        for elem in input {
            hasher.update(&elem.as_canonical_u32().to_le_bytes());
        }
        let hash = hasher.finalize();
        bytes_to_field_8(hash.as_bytes())
    }

    fn hash_iter_slices<'a, I: IntoIterator<Item = &'a [BabyBear]>>(
        &self,
        input: I,
    ) -> [BabyBear; 8] {
        let mut hasher = blake3::Hasher::new();
        for slice in input {
            for elem in slice {
                hasher.update(&elem.as_canonical_u32().to_le_bytes());
            }
        }
        let hash = hasher.finalize();
        bytes_to_field_8(hash.as_bytes())
    }
}

// ── Inner-Node Compressor ────────────────────────────────────────────────

impl PseudoCompressionFunction<[BabyBear; 8], 2> for Blake3FieldCompressor {
    fn compress(&self, input: [[BabyBear; 8]; 2]) -> [BabyBear; 8] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&field_8_to_bytes(&input[0]));
        hasher.update(&field_8_to_bytes(&input[1]));
        let hash = hasher.finalize();
        bytes_to_field_8(hash.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_symmetric::CryptographicHasher;

    #[test]
    fn blake3_field_hasher_deterministic() {
        let hasher = Blake3FieldHasher;
        let input = vec![BabyBear::from_u64(42), BabyBear::from_u64(7)];
        let h1 = hasher.hash_iter(input.clone());
        let h2 = hasher.hash_iter(input);
        assert_eq!(h1, h2);
    }

    #[test]
    fn blake3_field_hasher_different_inputs() {
        let hasher = Blake3FieldHasher;
        let h1 = hasher.hash_iter(vec![BabyBear::from_u64(1)]);
        let h2 = hasher.hash_iter(vec![BabyBear::from_u64(2)]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn blake3_compressor_deterministic() {
        let comp = Blake3FieldCompressor;
        let a = [BabyBear::from_u64(1); 8];
        let b = [BabyBear::from_u64(2); 8];
        let r1 = comp.compress([a, b]);
        let r2 = comp.compress([a, b]);
        assert_eq!(r1, r2);
    }

    #[test]
    fn bytes_roundtrip() {
        let fields = [
            BabyBear::from_u64(0),
            BabyBear::from_u64(1),
            BabyBear::from_u64(100),
            BabyBear::from_u64(1_000_000),
            BabyBear::from_u64(2_013_265_920), // p - 1
            BabyBear::from_u64(42),
            BabyBear::from_u64(7),
            BabyBear::from_u64(999),
        ];
        let bytes = field_8_to_bytes(&fields);
        let recovered = bytes_to_field_8(&bytes);
        assert_eq!(fields, recovered);
    }
}
