//! Poseidon2 KoalaBear constants and linear-layer helpers.
//!
//! Provides round constants, the internal diffusion diagonal, and
//! step-by-step permutation helpers for trace generation.
//!
//! KoalaBear Poseidon2 uses S-box degree d=3 (x^3) with 28 rounds
//! (4 full + 20 partial + 4 full).
//!
//! References:
//! - p3-koala-bear `poseidon2.rs` (round constants)
//! - p3-poseidon2 `external.rs` (`MDSMat4`, `mds_light_permutation`)
//! - p3-poseidon2 `internal.rs` (`matmul_internal`)

use core::ops::Neg;

use p3_field::{Field, PrimeCharacteristicRing};
use p3_koala_bear::KoalaBear;

/// Poseidon2 state width.
pub const WIDTH: usize = 16;

/// Number of initial full (external) rounds.
pub const INITIAL_FULL_ROUNDS: usize = 4;

/// Number of partial (internal) rounds.
pub const PARTIAL_ROUNDS: usize = 20;

/// Number of final full (external) rounds.
pub const FINAL_FULL_ROUNDS: usize = 4;

/// Total rounds per permutation: 4 + 20 + 4 = 28.
pub const TOTAL_ROUNDS: usize = INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS + FINAL_FULL_ROUNDS;

/// Returns true if round index `r` (0-based) is a full (external) round.
pub fn is_full_round(r: usize) -> bool {
    !(INITIAL_FULL_ROUNDS..INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS).contains(&r)
}

/// Get the round constants for round `r`.
///
/// Full rounds (r < 4 or r >= 24): 16-element vector.
/// Partial rounds (4 <= r < 24): only element 0 is nonzero.
pub fn round_constants(r: usize) -> [KoalaBear; WIDTH] {
    use p3_koala_bear::{
        KOALABEAR_POSEIDON2_RC_16_EXTERNAL_FINAL, KOALABEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL,
        KOALABEAR_POSEIDON2_RC_16_INTERNAL,
    };

    if r < INITIAL_FULL_ROUNDS {
        // Initial external round
        KOALABEAR_POSEIDON2_RC_16_EXTERNAL_INITIAL[r]
    } else if r < INITIAL_FULL_ROUNDS + PARTIAL_ROUNDS {
        // Internal round: only element 0 has a constant
        let mut rc = [KoalaBear::ZERO; WIDTH];
        rc[0] = KOALABEAR_POSEIDON2_RC_16_INTERNAL[r - INITIAL_FULL_ROUNDS];
        rc
    } else {
        // Final external round
        KOALABEAR_POSEIDON2_RC_16_EXTERNAL_FINAL[r - INITIAL_FULL_ROUNDS - PARTIAL_ROUNDS]
    }
}

/// Compute the internal diffusion diagonal (diag-minus-one values).
///
/// The internal linear layer is `(1 + Diag(V))`, where V is:
/// `[-2, 1, 2, 1/2, 3, 4, -1/2, -3, -4, 1/2^8, 1/8, 1/2^24, -1/2^8, -1/8, -1/16, -1/2^24]`
///
/// This function returns V as KoalaBear field elements.
pub fn internal_diag_minus_1() -> [KoalaBear; WIDTH] {
    let one = KoalaBear::ONE;
    let two = KoalaBear::TWO;
    [
        two.neg(),                   // -2
        one,                         // 1
        two,                         // 2
        two.inverse(),               // 1/2
        KoalaBear::from_u8(3),       // 3
        KoalaBear::from_u8(4),       // 4
        two.inverse().neg(),         // -1/2
        KoalaBear::from_u8(3).neg(), // -3
        KoalaBear::from_u8(4).neg(), // -4
        one.div_2exp_u64(8),         // 1/2^8
        one.div_2exp_u64(3),         // 1/8
        one.div_2exp_u64(24),        // 1/2^24
        one.div_2exp_u64(8).neg(),   // -1/2^8
        one.div_2exp_u64(3).neg(),   // -1/8
        one.div_2exp_u64(4).neg(),   // -1/16
        one.div_2exp_u64(24).neg(),  // -1/2^24
    ]
}

/// Apply the circ(2,3,1,1) MDS matrix to a 4-element vector.
///
/// Matrix: `[[2,3,1,1],[1,2,3,1],[1,1,2,3],[3,1,1,2]]`.
fn apply_mat4(x: &mut [KoalaBear; 4]) {
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
pub fn external_linear_layer(state: &mut [KoalaBear; WIDTH]) {
    // Apply M_4 to each block of 4
    for block in state.as_chunks_mut::<4>().0 {
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
pub fn internal_linear_layer(state: &mut [KoalaBear; WIDTH]) {
    let diag = internal_diag_minus_1();
    let sum: KoalaBear = state.iter().copied().sum();
    for i in 0..WIDTH {
        state[i] = state[i] * diag[i] + sum;
    }
}

/// Intermediate S-box values for trace generation.
pub struct SboxIntermediate {
    /// y = state + rc (S-box input).
    pub y: KoalaBear,
    /// y^2.
    pub y2: KoalaBear,
    /// y^3 = y * y^2 (S-box output for d=3).
    pub y3: KoalaBear,
    /// S-box output: y^3 (KoalaBear uses degree-3 S-box).
    pub out: KoalaBear,
}

/// Compute S-box with intermediate values (degree-3: x^3).
pub fn sbox_with_intermediates(y: KoalaBear) -> SboxIntermediate {
    let y2 = y * y;
    let y3 = y * y2;
    SboxIntermediate { y, y2, y3, out: y3 }
}

/// One round of Poseidon2, returning intermediate witness data.
///
/// `state` is modified in place (becomes the next round's input state).
/// Returns `(rc, sbox_y2, sbox_y3)` for each of the 16 elements.
pub fn poseidon2_round(
    state: &mut [KoalaBear; WIDTH],
    r: usize,
) -> ([KoalaBear; WIDTH], [KoalaBear; WIDTH], [KoalaBear; WIDTH]) {
    let rc = round_constants(r);
    let full = is_full_round(r);
    let mut y2_out = [KoalaBear::ZERO; WIDTH];
    let mut y3_out = [KoalaBear::ZERO; WIDTH];

    if full {
        // Full round: S-box on all elements
        let mut sbox_out = [KoalaBear::ZERO; WIDTH];
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
/// Returns a vector of 28 entries: `(state_before, rc, sbox_y2, sbox_y3)` per round,
/// plus the final output state.
pub fn poseidon2_permutation(
    input: [KoalaBear; WIDTH],
) -> (Vec<PoseidonRoundData>, [KoalaBear; WIDTH]) {
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
#[derive(Clone, Debug)]
pub struct PoseidonRoundData {
    /// State at the beginning of this round (before add_rc + sbox).
    pub state_before: [KoalaBear; WIDTH],
    /// Round constants (16 elements; partial rounds have rc[1..]=0).
    pub rc: [KoalaBear; WIDTH],
    /// S-box y^2 intermediates.
    pub sbox_y2: [KoalaBear; WIDTH],
    /// S-box y^3 intermediates.
    pub sbox_y3: [KoalaBear; WIDTH],
}
