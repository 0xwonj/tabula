//! Poseidon2 BabyBear constants and linear-layer helpers.
//!
//! Provides round constants, the internal diffusion diagonal, and
//! step-by-step permutation helpers for trace generation.
//!
//! References:
//! - p3-baby-bear `poseidon2.rs` (round constants)
//! - p3-poseidon2 `external.rs` (`MDSMat4`, `mds_light_permutation`)
//! - p3-poseidon2 `internal.rs` (`matmul_internal`)

use core::ops::Neg;

use p3_baby_bear::BabyBear;
use p3_field::{Field, PrimeCharacteristicRing};

/// Poseidon2 state width.
pub const WIDTH: usize = 16;

/// Number of initial full (external) rounds.
pub const INITIAL_FULL_ROUNDS: usize = 4;

/// Number of partial (internal) rounds.
pub const PARTIAL_ROUNDS: usize = 13;

/// Number of final full (external) rounds.
pub const FINAL_FULL_ROUNDS: usize = 4;

/// Total rounds per permutation: 4 + 13 + 4 = 21.
pub const TOTAL_ROUNDS: usize = INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS + FINAL_FULL_ROUNDS;

/// Returns true if round index `r` (0-based) is a full (external) round.
pub fn is_full_round(r: usize) -> bool {
    !(INITIAL_FULL_ROUNDS..INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS).contains(&r)
}

/// Get the round constants for round `r`.
///
/// Full rounds (r < 4 or r >= 17): 16-element vector.
/// Partial rounds (4 <= r < 17): only element 0 is nonzero.
pub fn round_constants(r: usize) -> [BabyBear; WIDTH] {
    use p3_baby_bear::{
        BABYBEAR_RC16_EXTERNAL_FINAL, BABYBEAR_RC16_EXTERNAL_INITIAL, BABYBEAR_RC16_INTERNAL,
    };

    if r < INITIAL_FULL_ROUNDS {
        // Initial external round
        BABYBEAR_RC16_EXTERNAL_INITIAL[r]
    } else if r < INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS {
        // Internal round: only element 0 has a constant
        let mut rc = [BabyBear::ZERO; WIDTH];
        rc[0] = BABYBEAR_RC16_INTERNAL[r - INITIAL_FULL_ROUNDS];
        rc
    } else {
        // Final external round
        BABYBEAR_RC16_EXTERNAL_FINAL[r - INITIAL_FULL_ROUNDS - PARTIAL_ROUNDS]
    }
}

/// Compute the internal diffusion diagonal (diag-minus-one values).
///
/// The internal linear layer is `(1 + Diag(V))`, where V is:
/// `[-2, 1, 2, 1/2, 3, 4, -1/2, -3, -4, 1/2^8, 1/4, 1/8, 1/2^27, -1/2^8, -1/16, -1/2^27]`
///
/// This function returns V as BabyBear field elements.
pub fn internal_diag_minus_1() -> [BabyBear; WIDTH] {
    let one = BabyBear::ONE;
    let two = BabyBear::TWO;
    [
        two.neg(),                  // -2
        one,                        // 1
        two,                        // 2
        two.inverse(),              // 1/2
        BabyBear::from_u8(3),       // 3
        BabyBear::from_u8(4),       // 4
        two.inverse().neg(),        // -1/2
        BabyBear::from_u8(3).neg(), // -3
        BabyBear::from_u8(4).neg(), // -4
        one.div_2exp_u64(8),        // 1/2^8
        one.div_2exp_u64(2),        // 1/4
        one.div_2exp_u64(3),        // 1/8
        one.div_2exp_u64(27),       // 1/2^27
        one.div_2exp_u64(8).neg(),  // -1/2^8
        one.div_2exp_u64(4).neg(),  // -1/16
        one.div_2exp_u64(27).neg(), // -1/2^27
    ]
}

/// Apply the circ(2,3,1,1) MDS matrix to a 4-element vector.
///
/// Matrix: `[[2,3,1,1],[1,2,3,1],[1,1,2,3],[3,1,1,2]]`.
fn apply_mat4(x: &mut [BabyBear; 4]) {
    let t01 = x[0] + x[1];
    let t23 = x[2] + x[3];
    let t0123 = t01 + t23;
    let t01123 = t0123 + x[1];
    let t01233 = t0123 + x[3];
    // Order matters: overwrite x[3], x[1] before x[0], x[2].
    x[3] = t01233 + x[0].double(); // 3*x[0] + x[1] + x[2] + 2*x[3]
    x[1] = t01123 + x[2].double(); // x[0] + 2*x[1] + 3*x[2] + x[3]
    x[0] = t01123 + t01; // 2*x[0] + 3*x[1] + x[2] + x[3]
    x[2] = t01233 + t23; // x[0] + x[1] + 2*x[2] + 3*x[3]
}

/// External linear layer (MDS) for full rounds.
///
/// Applies M_4 to each block of 4 elements, then column mixing.
pub fn external_linear_layer(state: &mut [BabyBear; WIDTH]) {
    // Apply M_4 to each block of 4
    for chunk in state.chunks_exact_mut(4) {
        let block: &mut [BabyBear; 4] = chunk.try_into().unwrap();
        apply_mat4(block);
    }
    // Column mixing: each element gets the sum of its column added
    for i in 0..4 {
        let col_sum = state[i] + state[i + 4] + state[i + 8] + state[i + 12];
        state[i] += col_sum;
        state[i + 4] += col_sum;
        state[i + 8] += col_sum;
        state[i + 12] += col_sum;
    }
}

/// Internal linear layer for partial rounds.
///
/// Computes `out[i] = state[i] * diag[i] + sum(state)`.
pub fn internal_linear_layer(state: &mut [BabyBear; WIDTH]) {
    let diag = internal_diag_minus_1();
    let sum: BabyBear = state.iter().copied().sum();
    for i in 0..WIDTH {
        state[i] = state[i] * diag[i] + sum;
    }
}

/// Intermediate S-box values for trace generation.
pub struct SboxIntermediate {
    /// y = state + rc (S-box input).
    pub y: BabyBear,
    /// y^2.
    pub y2: BabyBear,
    /// y^3.
    pub y3: BabyBear,
    /// y^7 = y^3 * (y^2)^2 (S-box output).
    pub out: BabyBear,
}

/// Compute S-box with intermediate values.
pub fn sbox_with_intermediates(y: BabyBear) -> SboxIntermediate {
    let y2 = y * y;
    let y3 = y * y2;
    let out = y3 * y2 * y2;
    SboxIntermediate { y, y2, y3, out }
}

/// One round of Poseidon2, returning intermediate witness data.
///
/// `state` is modified in place (becomes the next round's input state).
/// Returns `(rc, sbox_y2, sbox_y3)` for each of the 16 elements.
pub fn poseidon2_round(
    state: &mut [BabyBear; WIDTH],
    r: usize,
) -> ([BabyBear; WIDTH], [BabyBear; WIDTH], [BabyBear; WIDTH]) {
    let rc = round_constants(r);
    let full = is_full_round(r);
    let mut y2_out = [BabyBear::ZERO; WIDTH];
    let mut y3_out = [BabyBear::ZERO; WIDTH];

    if full {
        // Full round: S-box on all elements
        let mut sbox_out = [BabyBear::ZERO; WIDTH];
        for i in 0..WIDTH {
            let si = sbox_with_intermediates(state[i] + rc[i]);
            y2_out[i] = si.y2;
            y3_out[i] = si.y3;
            sbox_out[i] = si.out;
        }
        *state = sbox_out;
        external_linear_layer(state);
    } else {
        // Partial round: S-box only on element 0
        let si = sbox_with_intermediates(state[0] + rc[0]);
        y2_out[0] = si.y2;
        y3_out[0] = si.y3;
        state[0] = si.out;
        // Elements 1..15: identity S-box (sbox_y2/y3 are padding, set to 0)
        internal_linear_layer(state);
    }

    (rc, y2_out, y3_out)
}

/// Compute a full Poseidon2 permutation, returning all intermediate states.
///
/// Returns a vector of 21 entries: `(state_before, rc, sbox_y2, sbox_y3)` per round,
/// plus the final output state.
pub fn poseidon2_permutation(
    input: [BabyBear; WIDTH],
) -> (Vec<PoseidonRoundData>, [BabyBear; WIDTH]) {
    let mut state = input;

    // Initial external linear layer (pre-round MDS)
    external_linear_layer(&mut state);

    let mut rounds = Vec::with_capacity(TOTAL_ROUNDS);

    for r in 0..TOTAL_ROUNDS {
        let state_before = state;
        let (rc, y2, y3) = poseidon2_round(&mut state, r);
        rounds.push(PoseidonRoundData {
            state_before,
            rc,
            sbox_y2: y2,
            sbox_y3: y3,
        });
    }

    (rounds, state)
}

/// Per-round intermediate data for trace generation.
pub struct PoseidonRoundData {
    /// State at the beginning of this round (before add_rc + sbox).
    pub state_before: [BabyBear; WIDTH],
    /// Round constants (16 elements; partial rounds have rc[1..]=0).
    pub rc: [BabyBear; WIDTH],
    /// S-box y^2 intermediates.
    pub sbox_y2: [BabyBear; WIDTH],
    /// S-box y^3 intermediates.
    pub sbox_y3: [BabyBear; WIDTH],
}

#[cfg(test)]
mod tests {
    use p3_baby_bear::default_babybear_poseidon2_16;
    use p3_symmetric::Permutation;

    use super::*;

    #[test]
    fn round_constants_count() {
        assert_eq!(TOTAL_ROUNDS, 21);
        for r in 0..TOTAL_ROUNDS {
            let rc = round_constants(r);
            if is_full_round(r) {
                // Full round: all 16 constants should be nonzero (with high probability)
                assert!(rc.iter().any(|x| *x != BabyBear::ZERO));
            } else {
                // Partial round: only element 0 is nonzero, rest zero
                for (i, val) in rc.iter().enumerate().skip(1) {
                    assert_eq!(
                        *val,
                        BabyBear::ZERO,
                        "partial round {r} has nonzero rc[{i}]"
                    );
                }
            }
        }
    }

    #[test]
    fn internal_diag_nonzero() {
        let diag = internal_diag_minus_1();
        for (i, d) in diag.iter().enumerate() {
            assert_ne!(*d, BabyBear::ZERO, "diag[{i}] should be nonzero");
        }
    }

    #[test]
    fn permutation_matches_p3() {
        let p3_perm = default_babybear_poseidon2_16();

        let input: [BabyBear; 16] = core::array::from_fn(|i| BabyBear::new(i as u32 + 1));
        let mut p3_output = input;
        p3_perm.permute_mut(&mut p3_output);

        let (_, our_output) = poseidon2_permutation(input);
        assert_eq!(our_output, p3_output, "our Poseidon2 should match p3's");
    }

    #[test]
    fn sbox_correct() {
        let x = BabyBear::new(42);
        let si = sbox_with_intermediates(x);
        assert_eq!(si.y, x);
        assert_eq!(si.y2, x * x);
        assert_eq!(si.y3, x * x * x);
        // y^7 = y^3 * y^4 = y^3 * (y^2)^2
        let expected = x * x * x * (x * x) * (x * x);
        assert_eq!(si.out, expected);
    }
}
