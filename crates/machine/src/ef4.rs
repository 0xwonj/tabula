//! EF4 (BabyBear^4) arithmetic helpers.
//!
//! The quartic extension field uses the irreducible polynomial `X^4 - 11`.
//! These helpers decompose EF4 values into 4 BabyBear coefficients and perform
//! component-wise multiplication, used by the RAP constraint folders and
//! permutation trace generation.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use super::config::EF4;

// ─── EF4 arithmetic helpers ─────────────────────────────────────────────────

/// Extract 4 BabyBear coefficients from an EF4 value.
pub(crate) fn ef4_coeffs(val: EF4) -> [BabyBear; 4] {
    use p3_field::BasedVectorSpace;
    let s = val.as_basis_coefficients_slice();
    [s[0], s[1], s[2], s[3]]
}

/// Multiply two EF4 values decomposed into 4 components.
///
/// Generic over the component type — works for `BabyBear` (concrete precomputation),
/// `PackedVal<SC>` (prover constraints), and `EF4` (verifier constraints).
///
/// Uses the identity `X^4 = 11` in BabyBear^4 to reduce the product.
pub(crate) fn ef4_mul<T>(a: &[T; 4], b: &[T; 4]) -> [T; 4]
where
    T: PrimeCharacteristicRing + Copy,
{
    let w = T::from_u64(11);

    let c0 = a[0] * b[0] + w * (a[1] * b[3] + a[2] * b[2] + a[3] * b[1]);
    let c1 = a[0] * b[1] + a[1] * b[0] + w * (a[2] * b[3] + a[3] * b[2]);
    let c2 = a[0] * b[2] + a[1] * b[1] + a[2] * b[0] + w * (a[3] * b[3]);
    let c3 = a[0] * b[3] + a[1] * b[2] + a[2] * b[1] + a[3] * b[0];

    [c0, c1, c2, c3]
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::BasedVectorSpace;

    fn bb(x: u64) -> BabyBear {
        BabyBear::from_u64(x)
    }

    /// Verify that ef4_mul matches native EF4 multiplication.
    #[test]
    fn ef4_mul_matches_native() {
        let a = EF4::from_basis_coefficients_fn(|i| bb([3, 5, 7, 11][i]));
        let b = EF4::from_basis_coefficients_fn(|i| bb([2, 4, 6, 8][i]));
        let c = a * b;

        let a_arr = ef4_coeffs(a);
        let b_arr = ef4_coeffs(b);
        let c_arr = ef4_mul(&a_arr, &b_arr);

        let expected = ef4_coeffs(c);
        assert_eq!(c_arr, expected);
    }

    /// Verify ef4_coeffs round-trips correctly.
    #[test]
    fn ef4_coeffs_round_trip() {
        let coeffs = [bb(1), bb(2), bb(3), bb(4)];
        let val = EF4::from_basis_coefficients_fn(|i| coeffs[i]);
        assert_eq!(ef4_coeffs(val), coeffs);
    }
}
