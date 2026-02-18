use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use tabula_commitment::NativeDigest;

/// Create a deterministic distinct digest from a seed.
pub fn distinct_digest(seed: u32) -> NativeDigest {
    let mut fes = [BabyBear::ZERO; 8];
    for (i, fe) in fes.iter_mut().enumerate() {
        *fe = BabyBear::new(seed * 100 + i as u32);
    }
    NativeDigest(fes)
}
