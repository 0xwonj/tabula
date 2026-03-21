use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_commitment::{FieldHasher, NativeDigest};

/// Fast non-cryptographic field hasher for cross-crate tests.
///
/// This intentionally mirrors the commitment crate's internal mock hasher
/// without keeping it in the commitment public surface.
#[derive(Clone, Debug)]
pub struct MockFieldHasher;

impl FieldHasher for MockFieldHasher {
    type F = KoalaBear;
    type Digest = NativeDigest;

    fn hash(&self, input: &[KoalaBear]) -> NativeDigest {
        let mut state = [KoalaBear::ZERO; 8];
        for (i, &fe) in input.iter().enumerate() {
            let idx = i % 8;
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
