//! Column layout for the PoseidonChip AIR.
//!
//! One row per Poseidon2 round. 21 rows per permutation invocation.
//! State width is fixed at 16 (Poseidon2-BabyBear-16).

use crate::air::columns::num_cols;

use super::constants::WIDTH;

/// Column layout for the Poseidon2 AIR.
///
/// 69 columns total: state(16) + rc(16) + sbox_y2(16) + sbox_y3(16) + control(5).
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
#[repr(C)]
pub struct PoseidonCols<T> {
    // ── State (before add_rc + sbox) ──
    /// 16-element state at the beginning of this round.
    pub state: [T; WIDTH],

    // ── Round constants (witness, NOT constrained in M8) ──
    /// Round constants for this round. Full rounds: all 16 populated.
    /// Partial rounds: only rc[0] is nonzero.
    pub rc: [T; WIDTH],

    // ── S-box intermediates ──
    /// y^2 where y = state[i] + rc[i]. For partial rounds, only [0] is meaningful.
    pub sbox_y2: [T; WIDTH],
    /// y^3 = y * y^2. For partial rounds, only [0] is meaningful.
    pub sbox_y3: [T; WIDTH],

    // ── Round control ──
    /// Round index (0..20).
    pub round_ctr: T,
    /// 1 for full (external) rounds, 0 for partial (internal).
    pub is_full_round: T,
    /// 1 for round 0 of a permutation.
    pub is_first_round: T,
    /// 1 for round 20 (last round of a permutation).
    pub is_last_round: T,
    /// 1 for real rows, 0 for padding.
    pub is_real: T,

    // ── C5 PoseidonPermutation bus columns ──
    /// Raw (pre-MDS) permutation input. Constant within a permutation (21 rows).
    /// At first round: `state = external_linear_layer(perm_input)`.
    pub perm_input: [T; WIDTH],
    /// First 8 elements of the permutation output (Poseidon digest).
    /// Constant within a permutation (21 rows). Verified at last round:
    /// `perm_output = external_linear_layer(sbox_out)[0..8]`.
    pub perm_output: [T; 8],
}

/// Compute the width of PoseidonCols.
pub const fn poseidon_width() -> usize {
    num_cols::<PoseidonCols<u8>, u8>()
}

/// Width constant for PoseidonCols.
pub const POSEIDON_WIDTH: usize = poseidon_width();

/// Preprocessed column layout for the Poseidon2 AIR.
///
/// Contains the expected round constants and is_full_round flag for each row.
/// The prover cannot forge these values — they are fixed at setup time.
///
/// 17 columns: rc(16) + is_full_round(1).
#[repr(C)]
pub struct PoseidonPreprocessedCols<T> {
    /// Expected round constants for this row.
    pub rc: [T; WIDTH],
    /// Expected is_full_round flag for this row.
    pub is_full_round: T,
}

/// Width of the Poseidon preprocessed trace.
pub const POSEIDON_PREPROCESSED_WIDTH: usize = num_cols::<PoseidonPreprocessedCols<u8>, u8>();
