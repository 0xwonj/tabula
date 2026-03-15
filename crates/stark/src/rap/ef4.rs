//! EF4 (KoalaBear^4) arithmetic helpers.
//!
//! The quartic extension field uses the irreducible polynomial `X^4 - W`
//! where `W` is derived from [`KoalaBearParameters`]. These helpers decompose
//! EF4 values into 4 KoalaBear coefficients and perform component-wise
//! multiplication, used by the RAP constraint folders and permutation trace
//! generation.

use p3_field::{Algebra, PrimeCharacteristicRing};
use p3_koala_bear::{KoalaBear, KoalaBearParameters};
use p3_monty_31::BinomialExtensionData;

use crate::EF4;

// ─── EF4 arithmetic helpers ─────────────────────────────────────────────────

/// Extract 4 KoalaBear coefficients from an EF4 value.
pub fn ef4_coeffs(val: EF4) -> [KoalaBear; 4] {
    use p3_field::BasedVectorSpace;
    let s = val.as_basis_coefficients_slice();
    [s[0], s[1], s[2], s[3]]
}

/// Multiply two EF4 values decomposed into 4 components.
///
/// Generic over the component type — works for `KoalaBear` (concrete precomputation),
/// `PackedVal<SC>` (prover constraints), and `EF4` (verifier constraints).
///
/// Uses the identity `X^4 = W` (derived from [`KoalaBearParameters`]) to reduce
/// the product. The `mul_w` method is optimized per-field (e.g. `3a = 2a + a`
/// for KoalaBear where `W = 3`).
pub fn ef4_mul<T>(a: &[T; 4], b: &[T; 4]) -> [T; 4]
where
    T: Algebra<KoalaBear> + Copy,
{
    let mul_w = <KoalaBearParameters as BinomialExtensionData<4>>::mul_w::<T>;

    let c0 = a[0] * b[0] + mul_w(a[1] * b[3] + a[2] * b[2] + a[3] * b[1]);
    let c1 = a[0] * b[1] + a[1] * b[0] + mul_w(a[2] * b[3] + a[3] * b[2]);
    let c2 = a[0] * b[2] + a[1] * b[1] + a[2] * b[0] + mul_w(a[3] * b[3]);
    let c3 = a[0] * b[3] + a[1] * b[2] + a[2] * b[1] + a[3] * b[0];

    [c0, c1, c2, c3]
}

// ─── Shared RAP helpers ────────────────────────────────────────────────────
//
// Generic over `T` so that both the prover (`PackedVal<SC>`) and verifier
// (`EF4`) use the same algorithmic implementation — preventing constraint
// divergence between the two RAP folders.

/// Selector values for a single row evaluation.
///
/// Shared by [`RapProverFolder`](super::prover::RapProverFolder) and
/// [`RapVerifierFolder`](super::verifier::RapVerifierFolder) to guarantee
/// identical selector semantics.
#[derive(Clone, Copy)]
pub struct RowSelectors<T> {
    /// `1` on the first row, `0` elsewhere.
    pub is_first_row: T,
    /// `1` on the last row, `0` elsewhere.
    pub is_last_row: T,
    /// `1` on all rows except the last, `0` on the last.
    pub is_transition: T,
}

/// Compute the LogUp fingerprint components in decomposed EF4.
///
/// `f = α + tag + β·v[0] + β²·v[1] + …`
///
/// Returns the 4 KoalaBear-basis components of the fingerprint, evaluated
/// in the caller's expression type `T` (either `PackedVal` or `EF4`).
pub fn compute_fingerprint_components<T>(
    alpha_coeffs: [KoalaBear; 4],
    beta_coeffs: [KoalaBear; 4],
    tag: T,
    values: &[T],
) -> [T; 4]
where
    T: Algebra<KoalaBear> + Copy,
{
    let mut f: [T; 4] = [
        T::from(alpha_coeffs[0]) + tag,
        T::from(alpha_coeffs[1]),
        T::from(alpha_coeffs[2]),
        T::from(alpha_coeffs[3]),
    ];

    let mut beta_power = beta_coeffs;
    for val in values {
        for k in 0..4 {
            f[k] += T::from(beta_power[k]) * *val;
        }
        beta_power = ef4_mul(&beta_power, &beta_coeffs);
    }
    f
}

/// Compute the 12 cumsum constraint expressions.
///
/// Returns `[first_row × 4, transition × 4, last_row × 4]` in that order.
/// The caller feeds each value to its own `rap_assert_zero` method.
pub fn cumsum_constraint_values<T>(
    cumsum_local: [T; 4],
    cumsum_next: [T; 4],
    phi_sum_local: [T; 4],
    phi_sum_next: [T; 4],
    cumsum_final: [T; 4],
    sels: RowSelectors<T>,
) -> [T; 12]
where
    T: PrimeCharacteristicRing + Copy,
{
    let mut out = [T::ZERO; 12];
    // First row: cumsum[0] = phi_sum
    for k in 0..4 {
        out[k] = sels.is_first_row * (cumsum_local[k] - phi_sum_local[k]);
    }
    // Transition: cumsum_next = cumsum_local + phi_sum_next
    for k in 0..4 {
        out[4 + k] = sels.is_transition * (cumsum_next[k] - cumsum_local[k] - phi_sum_next[k]);
    }
    // Last row: cumsum = cumsum_final (binds to proof value for cross-chip balance)
    for k in 0..4 {
        out[8 + k] = sels.is_last_row * (cumsum_local[k] - cumsum_final[k]);
    }
    out
}

/// Build alpha power vectors for constraint folding.
///
/// Returns `(alpha_powers, decomposed_alpha_powers)` where `alpha_powers`
/// is in reverse order (highest power first) for Horner's method accumulation,
/// and `decomposed_alpha_powers` splits each EF4 into its 4 KoalaBear basis
/// components (required by p3's `ProverConstraintFolder`).
pub fn build_alpha_powers(alpha: EF4, count: usize) -> (Vec<EF4>, Vec<Vec<KoalaBear>>) {
    use p3_field::BasedVectorSpace;

    let mut alpha_powers = Vec::with_capacity(count);
    let mut power = EF4::ONE;
    for _ in 0..count {
        alpha_powers.push(power);
        power *= alpha;
    }
    alpha_powers.reverse();

    let decomposed: Vec<Vec<KoalaBear>> = (0..<EF4 as BasedVectorSpace<KoalaBear>>::DIMENSION)
        .map(|i| {
            alpha_powers
                .iter()
                .map(|x| <EF4 as BasedVectorSpace<KoalaBear>>::as_basis_coefficients_slice(x)[i])
                .collect()
        })
        .collect();

    (alpha_powers, decomposed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::BasedVectorSpace;

    fn bb(x: u64) -> KoalaBear {
        KoalaBear::from_u64(x)
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
