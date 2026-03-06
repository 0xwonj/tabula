//! RAP constraint folder for the prover phase.

use p3_air::{AirBuilder, AirBuilderWithPublicValues, PairBuilder};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrixView;
use p3_uni_stark::{PackedChallenge, PackedVal, StarkGenericConfig};

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::interaction::AirInteraction;

use crate::config::{EF4, TabulaStarkConfig};
use crate::ef4::{ef4_coeffs, ef4_mul};

type SC = TabulaStarkConfig;
type PV = PackedVal<SC>;
type PC = PackedChallenge<SC>;

/// Constraint folder for Phase 2 (RAP constraints) of the prover.
///
/// When the chip's `eval()` is called with this folder:
/// - `assert_zero()` is a no-op (main constraints handled in Phase 1)
/// - `send()`/`receive()` generate RAP constraints from actual interaction values
///
/// After `eval()` returns, call [`finalize_cumsum`](Self::finalize_cumsum) to
/// add cumsum transition constraints.
pub struct RapProverFolder<'a> {
    /// Truncated main trace view (main columns only) — returned by `AirBuilder::main()`.
    main_truncated: RowMajorMatrixView<'a, PV>,
    /// Full combined trace view (main ∥ perm) — used internally for phi access.
    full_trace: RowMajorMatrixView<'a, PV>,
    /// Preprocessed columns (if any).
    preprocessed: Option<RowMajorMatrixView<'a, PV>>,
    /// Public values.
    public_values: &'a [BabyBear],
    /// Selectors.
    is_first_row: PV,
    is_last_row: PV,
    is_transition: PV,
    /// Alpha powers for constraint folding (shared with Phase 1).
    alpha_powers: &'a [<SC as StarkGenericConfig>::Challenge],
    /// Running accumulator (starts from Phase 1's accumulator).
    accumulator: PC,
    /// Current constraint index (starts from Phase 1's constraint_index).
    constraint_index: usize,

    // ── RAP-specific fields ──
    /// LogUp challenges [α, β] in EF4.
    challenges: [EF4; 2],
    /// Width of the main trace (perm columns start at this offset).
    main_width: usize,
    /// Current interaction index (0-based).
    interaction_index: usize,
    /// Accumulated signed phi sums for cumsum constraints (local row).
    phi_sum_local: [PV; 4],
    /// Accumulated signed phi sums for cumsum constraints (next row).
    phi_sum_next: [PV; 4],
}

impl<'a> RapProverFolder<'a> {
    /// Create a new RAP prover folder.
    ///
    /// `accumulator` and `constraint_index` should be taken from the Phase 1
    /// folder after `chip.eval()` returns.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        main_truncated: RowMajorMatrixView<'a, PV>,
        full_trace: RowMajorMatrixView<'a, PV>,
        preprocessed: Option<RowMajorMatrixView<'a, PV>>,
        public_values: &'a [BabyBear],
        is_first_row: PV,
        is_last_row: PV,
        is_transition: PV,
        alpha_powers: &'a [<SC as StarkGenericConfig>::Challenge],
        accumulator: PC,
        constraint_index: usize,
        challenges: [EF4; 2],
        main_width: usize,
    ) -> Self {
        Self {
            main_truncated,
            full_trace,
            preprocessed,
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            alpha_powers,
            accumulator,
            constraint_index,
            challenges,
            main_width,
            interaction_index: 0,
            phi_sum_local: [PV::ZERO; 4],
            phi_sum_next: [PV::ZERO; 4],
        }
    }

    /// The running accumulator after constraint folding.
    pub(crate) fn accumulator(&self) -> PC {
        self.accumulator
    }

    /// The current constraint index (number of constraints folded so far).
    pub(crate) fn constraint_index(&self) -> usize {
        self.constraint_index
    }

    /// Accumulate a RAP constraint with the current alpha power.
    fn rap_assert_zero(&mut self, x: PV) {
        let alpha_pc: PC = self.alpha_powers[self.constraint_index].into();
        self.accumulator += alpha_pc * x;
        self.constraint_index += 1;
    }

    /// Generate cumsum constraints (call after chip.eval() returns).
    ///
    /// First row: `cumsum[0] = Σ(±phi_j)`
    /// Transition: `cumsum_next = cumsum_local + Σ(±phi_j_next)`
    pub fn finalize_cumsum(&mut self) {
        let trace = self.full_trace;
        let local_row = trace.row_slice(0).expect("row exists");
        let next_row = trace.row_slice(1).expect("row exists");

        let cumsum_offset = self.main_width + self.interaction_index * 4;

        let cumsum_local: [PV; 4] = [
            local_row[cumsum_offset],
            local_row[cumsum_offset + 1],
            local_row[cumsum_offset + 2],
            local_row[cumsum_offset + 3],
        ];
        let cumsum_next: [PV; 4] = [
            next_row[cumsum_offset],
            next_row[cumsum_offset + 1],
            next_row[cumsum_offset + 2],
            next_row[cumsum_offset + 3],
        ];

        let phi_sum_local = self.phi_sum_local;
        let phi_sum_next = self.phi_sum_next;

        for k in 0..4 {
            self.rap_assert_zero(self.is_first_row * (cumsum_local[k] - phi_sum_local[k]));
        }

        for k in 0..4 {
            self.rap_assert_zero(
                self.is_transition * (cumsum_next[k] - cumsum_local[k] - phi_sum_next[k]),
            );
        }
    }

    /// Process a single interaction (shared logic for send/receive).
    fn process_interaction(&mut self, interaction: &AirInteraction<PV>, is_send: bool) {
        let trace = self.full_trace;
        let local_row = trace.row_slice(0).expect("row exists");
        let next_row = trace.row_slice(1).expect("row exists");

        let phi_offset = self.main_width + self.interaction_index * 4;

        let phi_local: [PV; 4] = [
            local_row[phi_offset],
            local_row[phi_offset + 1],
            local_row[phi_offset + 2],
            local_row[phi_offset + 3],
        ];

        let [alpha, beta] = self.challenges;
        let alpha_coeffs = ef4_coeffs(alpha);
        let beta_coeffs = ef4_coeffs(beta);

        let tag = PV::from(BabyBear::from_u64(interaction.bus.tag() as u64));
        let mut f: [PV; 4] = [
            PV::from(alpha_coeffs[0]) + tag,
            PV::from(alpha_coeffs[1]),
            PV::from(alpha_coeffs[2]),
            PV::from(alpha_coeffs[3]),
        ];

        let mut beta_power = beta_coeffs;
        for val in &interaction.values {
            for k in 0..4 {
                f[k] += PV::from(beta_power[k]) * *val;
            }
            beta_power = ef4_mul(&beta_power, &beta_coeffs);
        }

        let product = ef4_mul(&phi_local, &f);
        let m = interaction.multiplicity;
        self.rap_assert_zero(product[0] - m);
        self.rap_assert_zero(product[1]);
        self.rap_assert_zero(product[2]);
        self.rap_assert_zero(product[3]);

        let phi_next: [PV; 4] = [
            next_row[phi_offset],
            next_row[phi_offset + 1],
            next_row[phi_offset + 2],
            next_row[phi_offset + 3],
        ];

        if is_send {
            for k in 0..4 {
                self.phi_sum_local[k] += phi_local[k];
                self.phi_sum_next[k] += phi_next[k];
            }
        } else {
            for k in 0..4 {
                self.phi_sum_local[k] -= phi_local[k];
                self.phi_sum_next[k] -= phi_next[k];
            }
        }

        self.interaction_index += 1;
    }
}

impl<'a> AirBuilder for RapProverFolder<'a> {
    type F = BabyBear;
    type Expr = PV;
    type Var = PV;
    type M = RowMajorMatrixView<'a, PV>;

    fn main(&self) -> Self::M {
        self.main_truncated
    }

    fn is_first_row(&self) -> Self::Expr {
        self.is_first_row
    }

    fn is_last_row(&self) -> Self::Expr {
        self.is_last_row
    }

    fn is_transition_window(&self, _size: usize) -> Self::Expr {
        self.is_transition
    }

    fn assert_zero<I: Into<Self::Expr>>(&mut self, _x: I) {
        // No-op: main constraints are handled in Phase 1.
    }
}

impl<'a> PairBuilder for RapProverFolder<'a> {
    fn preprocessed(&self) -> Self::M {
        self.preprocessed
            .unwrap_or_else(|| RowMajorMatrixView::new(&[], 0))
    }
}

impl<'a> AirBuilderWithPublicValues for RapProverFolder<'a> {
    type PublicVar = BabyBear;

    fn public_values(&self) -> &[Self::PublicVar] {
        self.public_values
    }
}

impl<'a> InteractionAirBuilder for RapProverFolder<'a> {
    fn send(&mut self, interaction: AirInteraction<Self::Expr>) {
        self.process_interaction(&interaction, true);
    }

    fn receive(&mut self, interaction: AirInteraction<Self::Expr>) {
        self.process_interaction(&interaction, false);
    }
}
