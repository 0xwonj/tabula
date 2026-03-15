//! Shared test value constructors.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;

/// Create a deterministic distinct digest from a seed.
pub fn distinct_digest(seed: u32) -> NativeDigest {
    let mut fes = [KoalaBear::ZERO; 8];
    for (i, fe) in fes.iter_mut().enumerate() {
        *fe = KoalaBear::new(seed * 100 + i as u32);
    }
    NativeDigest(fes)
}

/// Compute `Com_empty = Poseidon(0x00 || t || c || 0..)` — the canonical empty-column commitment.
///
/// This is the protocol-defined commitment for an empty column, verified by the
/// ColumnMeta AIR's `constrain_com_empty` constraint.
pub fn com_empty(table: u32, col: u16) -> NativeDigest {
    use crate::poseidon::constants::poseidon2_permutation;
    let mut input = [KoalaBear::ZERO; 16];
    // input[0] = 0x00 (SSMC domain tag) — already zero
    input[1] = KoalaBear::new(table);
    input[2] = KoalaBear::new(col as u32);
    let (_rounds, output) = poseidon2_permutation(input);
    NativeDigest(core::array::from_fn(|i| output[i]))
}
