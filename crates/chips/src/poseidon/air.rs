//! PoseidonChip — AIR constraints for the Poseidon2 permutation.
//!
//! One row per round, 21 rows per permutation. Constraints enforce:
//! 1. Boolean fields: is_real, is_full_round, is_first_round, is_last_round
//! 2. `is_real` prefix: monotonic 1→0
//! 3. S-box decomposition: y2 = (state+rc)^2, y3 = (state+rc)*y2
//! 4. Linear layer: next.state = ext_linear(sbox_out) or int_linear(sbox_out)
//! 5. Round control: counter increment, permutation boundaries
//!
//! NOT constrained in M8 (deferred to M9):
//! - rc values correct per round (needs preprocessed columns)
//! - is_full_round consistent with round_ctr
//! - LogUp bus interactions

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::columns::{PoseidonCols, PoseidonPreprocessedCols, poseidon_width};
use super::constants::WIDTH;

/// The Poseidon2 AIR chip.
#[derive(Debug)]
pub struct PoseidonChip;

impl<F> BaseAir<F> for PoseidonChip {
    fn width(&self) -> usize {
        poseidon_width()
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for PoseidonChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &PoseidonCols<AB::Var> = borrow_cols(&local_row);
        let next: &PoseidonCols<AB::Var> = borrow_cols(&next_row);

        let is_real: AB::Expr = local.is_real.clone().into();
        let is_full: AB::Expr = local.is_full_round.clone().into();
        let not_full: AB::Expr = AB::Expr::ONE - is_full.clone();
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();

        let not_last: AB::Expr = AB::Expr::ONE - local.is_last_round.clone().into();

        constrain_booleans(builder, local);
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());
        constrain_sbox_element0(builder, local, is_real.clone());
        constrain_sbox_full_round(builder, local, is_real.clone(), is_full.clone());
        // Linear layer transitions: NOT applied to last round (row 20 → next permutation or padding).
        let layer_gate_full: AB::Expr = is_real.clone() * is_full.clone() * not_last.clone();
        let layer_gate_partial: AB::Expr = is_real.clone() * not_full * not_last;
        constrain_linear_layer_full(builder, local, next, layer_gate_full);
        constrain_linear_layer_partial(builder, local, next, layer_gate_partial);
        constrain_round_control(builder, local, next, both_real.clone());
        constrain_perm_output(builder, local, next, is_real.clone(), both_real);
        constrain_round_constants(builder, local, is_real.clone());

        // ── LogUp bus ──
        receive_poseidon_permutation(builder, local);
    }
}

// ── Private constraint helpers ──────────────────────────────────────────────

/// 1. Boolean constraints.
fn constrain_booleans<AB: AirBuilder>(builder: &mut AB, local: &PoseidonCols<AB::Var>) {
    builder.assert_bool(local.is_full_round.clone());
    builder.assert_bool(local.is_first_round.clone());
    builder.assert_bool(local.is_last_round.clone());
}

/// 3a. S-box decomposition for element 0 (always active when is_real).
///
/// y = state[0] + rc[0]
/// sbox_y2[0] = y^2
/// sbox_y3[0] = y * sbox_y2[0]
fn constrain_sbox_element0<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    is_real: AB::Expr,
) {
    let y: AB::Expr = local.state[0].clone().into() + local.rc[0].clone().into();
    let y2: AB::Expr = local.sbox_y2[0].clone().into();
    let y3: AB::Expr = local.sbox_y3[0].clone().into();

    // sbox_y2[0] = y^2
    builder
        .when(is_real.clone())
        .assert_zero(y2.clone() - y.clone() * y.clone());

    // sbox_y3[0] = y * sbox_y2[0]
    builder.when(is_real).assert_zero(y3 - y * y2);
}

/// 3b. S-box decomposition for elements 1..15 (gated by is_full_round).
fn constrain_sbox_full_round<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    is_real: AB::Expr,
    is_full: AB::Expr,
) {
    let gate: AB::Expr = is_real * is_full;

    for i in 1..WIDTH {
        let y: AB::Expr = local.state[i].clone().into() + local.rc[i].clone().into();
        let y2: AB::Expr = local.sbox_y2[i].clone().into();
        let y3: AB::Expr = local.sbox_y3[i].clone().into();

        // sbox_y2[i] = y^2
        builder
            .when(gate.clone())
            .assert_zero(y2.clone() - y.clone() * y.clone());

        // sbox_y3[i] = y * sbox_y2[i]
        builder.when(gate.clone()).assert_zero(y3 - y * y2);
    }
}

/// 4a. Linear layer for full (external) rounds.
///
/// Transition constraint: next.state = external_linear_layer(sbox_out)
/// where sbox_out[i] = sbox_y3[i] * sbox_y2[i]^2 = y^7.
///
/// `gate` = `is_real * is_full * (1 - is_last_round)`.
fn constrain_linear_layer_full<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    next: &PoseidonCols<AB::Var>,
    gate: AB::Expr,
) {
    // Compute sbox_out[i] = sbox_y3[i] * sbox_y2[i]^2 (degree 3)
    let sbox_out: [AB::Expr; WIDTH] = core::array::from_fn(|i| {
        let y2: AB::Expr = local.sbox_y2[i].clone().into();
        let y3: AB::Expr = local.sbox_y3[i].clone().into();
        y3 * y2.clone() * y2
    });

    // Apply external linear layer (MDS) to get expected next state
    let expected = external_linear_exprs::<AB>(sbox_out);

    for (i, exp) in expected.iter().enumerate() {
        builder
            .when_transition()
            .when(gate.clone())
            .assert_zero(next.state[i].clone().into() - exp.clone());
    }
}

/// 4b. Linear layer for partial (internal) rounds.
///
/// Transition constraint: next.state = internal_linear_layer(sbox_out)
/// where sbox_out[0] = y[0]^7, sbox_out[i] = state[i] + rc[i] for i > 0.
///
/// `gate` = `is_real * (1 - is_full) * (1 - is_last_round)`.
fn constrain_linear_layer_partial<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    next: &PoseidonCols<AB::Var>,
    gate: AB::Expr,
) {
    // Element 0: full S-box
    let y2_0: AB::Expr = local.sbox_y2[0].clone().into();
    let y3_0: AB::Expr = local.sbox_y3[0].clone().into();
    let sbox_out_0: AB::Expr = y3_0 * y2_0.clone() * y2_0;

    // Elements 1..15: identity S-box (pass through state + rc)
    let mut sbox_out: [AB::Expr; WIDTH] =
        core::array::from_fn(|i| local.state[i].clone().into() + local.rc[i].clone().into());
    sbox_out[0] = sbox_out_0;

    // Apply internal linear layer
    let diag = internal_diag_exprs::<AB>();
    let expected = internal_linear_exprs::<AB>(sbox_out, &diag);

    for (i, exp) in expected.iter().enumerate() {
        builder
            .when_transition()
            .when(gate.clone())
            .assert_zero(next.state[i].clone().into() - exp.clone());
    }
}

/// 5. Round control constraints.
fn constrain_round_control<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    next: &PoseidonCols<AB::Var>,
    both_real: AB::Expr,
) {
    let is_last: AB::Expr = local.is_last_round.clone().into();
    let not_last: AB::Expr = AB::Expr::ONE - is_last.clone();

    // Counter increment: when both real and NOT last round, counter increments by 1
    builder.when_transition().assert_zero(
        both_real.clone()
            * not_last.clone()
            * (next.round_ctr.clone().into() - local.round_ctr.clone().into() - AB::Expr::ONE),
    );

    // After last round: next must be first round (new permutation) or not real
    // is_last * next.is_real * (1 - next.is_first_round) = 0
    builder.when_transition().assert_zero(
        is_last
            * next.is_real.clone().into()
            * (AB::Expr::ONE - next.is_first_round.clone().into()),
    );

    // After last round, if both real: next.round_ctr = 0
    builder.when_transition().assert_zero(
        both_real * local.is_last_round.clone().into() * next.round_ctr.clone().into(),
    );
}

// ── Linear layer expression helpers ─────────────────────────────────────────

/// Apply the circ(2,3,1,1) MDS matrix to 4 expressions.
///
/// Returns [2a+3b+c+d, a+2b+3c+d, a+b+2c+3d, 3a+b+c+2d].
fn apply_mat4_exprs<AB: AirBuilder>(x: [AB::Expr; 4]) -> [AB::Expr; 4] {
    let t01 = x[0].clone() + x[1].clone();
    let t23 = x[2].clone() + x[3].clone();
    let t0123 = t01.clone() + t23.clone();
    let t01123 = t0123.clone() + x[1].clone();
    let t01233 = t0123 + x[3].clone();
    [
        t01123.clone() + t01,           // 2*x[0] + 3*x[1] + x[2] + x[3]
        t01123 + x[2].clone().double(), // x[0] + 2*x[1] + 3*x[2] + x[3]
        t01233.clone() + t23,           // x[0] + x[1] + 2*x[2] + 3*x[3]
        t01233 + x[0].clone().double(), // 3*x[0] + x[1] + x[2] + 2*x[3]
    ]
}

/// External linear layer (MDS) in expression form.
///
/// Applies M_4 to each block of 4, then column mixing.
fn external_linear_exprs<AB: AirBuilder>(input: [AB::Expr; WIDTH]) -> [AB::Expr; WIDTH] {
    // Apply M_4 to each block of 4
    let mut result = input;
    for chunk in 0..4 {
        let base = chunk * 4;
        let block = [
            result[base].clone(),
            result[base + 1].clone(),
            result[base + 2].clone(),
            result[base + 3].clone(),
        ];
        let out = apply_mat4_exprs::<AB>(block);
        result[base] = out[0].clone();
        result[base + 1] = out[1].clone();
        result[base + 2] = out[2].clone();
        result[base + 3] = out[3].clone();
    }

    // Column mixing: add column sum to each element
    for i in 0..4 {
        let col_sum = result[i].clone()
            + result[i + 4].clone()
            + result[i + 8].clone()
            + result[i + 12].clone();
        result[i] = result[i].clone() + col_sum.clone();
        result[i + 4] = result[i + 4].clone() + col_sum.clone();
        result[i + 8] = result[i + 8].clone() + col_sum.clone();
        result[i + 12] = result[i + 12].clone() + col_sum;
    }

    result
}

/// Compute the internal diffusion diagonal as `AB::F` values.
///
/// V = [-2, 1, 2, 1/2, 3, 4, -1/2, -3, -4, 1/2^8, 1/4, 1/8, 1/2^27, -1/2^8, -1/16, -1/2^27].
/// Uses only `PrimeCharacteristicRing` methods (no `Field::inverse`).
fn internal_diag_exprs<AB: AirBuilder>() -> [AB::F; WIDTH] {
    let one = AB::F::ONE;
    let two = AB::F::TWO;
    let neg_one = AB::F::NEG_ONE;
    let half = one.clone().halve();
    [
        AB::F::ZERO - two.clone(),                     // -2
        one.clone(),                                   // 1
        two,                                           // 2
        half.clone(),                                  // 1/2
        AB::F::from_u8(3),                             // 3
        AB::F::from_u8(4),                             // 4
        AB::F::ZERO - half,                            // -1/2
        AB::F::ZERO - AB::F::from_u8(3),               // -3
        AB::F::ZERO - AB::F::from_u8(4),               // -4
        one.clone().div_2exp_u64(8),                   // 1/2^8
        one.clone().div_2exp_u64(2),                   // 1/4
        one.clone().div_2exp_u64(3),                   // 1/8
        one.clone().div_2exp_u64(27),                  // 1/2^27
        neg_one.clone() * one.clone().div_2exp_u64(8), // -1/2^8
        neg_one.clone() * one.clone().div_2exp_u64(4), // -1/16
        neg_one * one.div_2exp_u64(27),                // -1/2^27
    ]
}

/// Internal linear layer in expression form.
///
/// Computes `out[i] = input[i] * diag[i] + sum(input)`.
fn internal_linear_exprs<AB: AirBuilder>(
    input: [AB::Expr; WIDTH],
    diag: &[AB::F; WIDTH],
) -> [AB::Expr; WIDTH] {
    let sum: AB::Expr = input.iter().cloned().sum();
    core::array::from_fn(|i| input[i].clone() * diag[i].clone() + sum.clone())
}

// ── perm_output constraints ─────────────────────────────────────────────────

/// Constrain `perm_input` and `perm_output` — carry + verification.
///
/// **perm_input** (raw pre-MDS input):
/// 1. Carry: constant within a permutation (not last round → next equals local).
/// 2. First-round verification: `state = external_linear_layer(perm_input)`.
///
/// **perm_output** (first 8 elements of permutation output):
/// 3. Carry: constant within a permutation.
/// 4. Last-round verification: `perm_output = external_linear_layer(sbox_out)[0..8]`.
fn constrain_perm_output<AB: AirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    next: &PoseidonCols<AB::Var>,
    is_real: AB::Expr,
    both_real: AB::Expr,
) {
    let not_last: AB::Expr = AB::Expr::ONE - local.is_last_round.clone().into();

    // 1-3. Carry: perm_input and perm_output constant within a permutation.
    let carry_gate: AB::Expr = both_real * not_last;
    for i in 0..WIDTH {
        let diff: AB::Expr = next.perm_input[i].clone().into() - local.perm_input[i].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }
    for j in 0..8 {
        let diff: AB::Expr =
            next.perm_output[j].clone().into() - local.perm_output[j].clone().into();
        builder
            .when_transition()
            .assert_zero(carry_gate.clone() * diff);
    }

    // 2. First-round verification: state = external_linear_layer(perm_input).
    let first_gate: AB::Expr = is_real.clone() * local.is_first_round.clone().into();
    let perm_input_exprs: [AB::Expr; WIDTH] =
        core::array::from_fn(|i| local.perm_input[i].clone().into());
    let expected_state = external_linear_exprs::<AB>(perm_input_exprs);
    for (i, exp) in expected_state.iter().enumerate() {
        builder.assert_zero(first_gate.clone() * (local.state[i].clone().into() - exp.clone()));
    }

    // 4. Last-round verification: perm_output = external_linear_layer(sbox_out)[0..8].
    let sbox_out: [AB::Expr; WIDTH] = core::array::from_fn(|i| {
        let y2: AB::Expr = local.sbox_y2[i].clone().into();
        let y3: AB::Expr = local.sbox_y3[i].clone().into();
        y3 * y2.clone() * y2
    });
    let expected_output = external_linear_exprs::<AB>(sbox_out);

    let verify_gate: AB::Expr = is_real * local.is_last_round.clone().into();
    for (j, exp) in expected_output.iter().enumerate().take(8) {
        builder
            .assert_zero(verify_gate.clone() * (local.perm_output[j].clone().into() - exp.clone()));
    }
}

// ── Preprocessed round constant verification ─────────────────────────────────

/// 6. Round constant verification via preprocessed columns.
///
/// Constrains `rc[i]` and `is_full_round` in the main trace to match
/// the preprocessed values. This prevents the prover from forging
/// round constants to produce arbitrary hash outputs.
///
/// For each i: `is_real * (main.rc[i] - prep.rc[i]) = 0`
/// And: `is_real * (main.is_full_round - prep.is_full_round) = 0`
///
/// When no preprocessed trace is provided (zero-width), round constant
/// constraints are skipped — only chips with preprocessed data enforce them.
fn constrain_round_constants<AB: InteractionAirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
    is_real: AB::Expr,
) {
    let prep = builder.preprocessed();

    // Zero-width preprocessed → height=0 → row_slice returns None → skip.
    if let Some(prep_row) = prep.row_slice(0) {
        let prep: &PoseidonPreprocessedCols<AB::Var> = borrow_cols(&prep_row);

        for i in 0..WIDTH {
            builder.assert_zero(
                is_real.clone() * (local.rc[i].clone().into() - prep.rc[i].clone().into()),
            );
        }
        builder.assert_zero(
            is_real.clone()
                * (local.is_full_round.clone().into() - prep.is_full_round.clone().into()),
        );
        builder.assert_zero(
            is_real.clone()
                * (local.is_first_round.clone().into() - prep.is_first_round.clone().into()),
        );
        builder.assert_zero(
            is_real * (local.is_last_round.clone().into() - prep.is_last_round.clone().into()),
        );
    }
}

// ── LogUp bus interaction ───────────────────────────────────────────────────

/// C5 PoseidonPermutation bus receive.
///
/// Tuple: `(perm_input[0..16], perm_output[0..8])` — 24 elements.
/// Multiplicity: `is_real · is_first_round`.
///
/// Receives at the first row of each permutation. `perm_input` is the raw
/// (pre-MDS) permutation input, `perm_output` is the verified digest.
fn receive_poseidon_permutation<AB: InteractionAirBuilder>(
    builder: &mut AB,
    local: &PoseidonCols<AB::Var>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.is_first_round.clone().into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for i in 0..WIDTH {
        values.push(local.perm_input[i].clone().into());
    }
    for j in 0..8 {
        values.push(local.perm_output[j].clone().into());
    }

    builder.receive(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::POSEIDON_PERM,
    });
}
