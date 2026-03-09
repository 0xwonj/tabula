//! RAP constraint folder for the verifier phase.

use p3_air::{AirBuilder, AirBuilderWithPublicValues, PairBuilder};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrixView;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::interaction::AirInteraction;

use crate::config::EF4;
use crate::ef4::{
    RowSelectors, compute_fingerprint_components, cumsum_constraint_values, ef4_coeffs, ef4_mul,
};

/// Constraint folder for Phase 2 (RAP constraints) of the verifier.
///
/// Same pattern as [`super::RapProverFolder`] but for the verifier's scalar types.
pub struct RapVerifierFolder<'a> {
    /// Truncated main trace (main columns only) — returned by `AirBuilder::main()`.
    main_truncated:
        p3_matrix::stack::VerticalPair<RowMajorMatrixView<'a, EF4>, RowMajorMatrixView<'a, EF4>>,
    /// Full combined trace (main ∥ perm) — used internally for phi access.
    full_trace:
        p3_matrix::stack::VerticalPair<RowMajorMatrixView<'a, EF4>, RowMajorMatrixView<'a, EF4>>,
    /// Preprocessed OOD values (if any).
    preprocessed: Option<
        p3_matrix::stack::VerticalPair<RowMajorMatrixView<'a, EF4>, RowMajorMatrixView<'a, EF4>>,
    >,
    /// Public values.
    public_values: &'a [BabyBear],
    /// Selectors at OOD point.
    is_first_row: EF4,
    is_last_row: EF4,
    is_transition: EF4,
    /// Alpha challenge for Horner accumulation.
    alpha: EF4,
    /// Running accumulator (starts from Phase 1's accumulator).
    accumulator: EF4,

    // ── RAP-specific fields ──
    /// LogUp challenges [α, β] in EF4.
    challenges: [EF4; 2],
    /// Width of the main trace (perm columns start at this offset).
    main_width: usize,
    /// Current interaction index (0-based).
    interaction_index: usize,
    /// Accumulated signed phi sums for cumsum constraints (local row).
    phi_sum_local: [EF4; 4],
    /// Accumulated signed phi sums for cumsum constraints (next row).
    phi_sum_next: [EF4; 4],
}

impl<'a> RapVerifierFolder<'a> {
    /// Create a new RAP verifier folder.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        main_truncated: p3_matrix::stack::VerticalPair<
            RowMajorMatrixView<'a, EF4>,
            RowMajorMatrixView<'a, EF4>,
        >,
        full_trace: p3_matrix::stack::VerticalPair<
            RowMajorMatrixView<'a, EF4>,
            RowMajorMatrixView<'a, EF4>,
        >,
        preprocessed: Option<
            p3_matrix::stack::VerticalPair<
                RowMajorMatrixView<'a, EF4>,
                RowMajorMatrixView<'a, EF4>,
            >,
        >,
        public_values: &'a [BabyBear],
        is_first_row: EF4,
        is_last_row: EF4,
        is_transition: EF4,
        alpha: EF4,
        accumulator: EF4,
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
            alpha,
            accumulator,
            challenges,
            main_width,
            interaction_index: 0,
            phi_sum_local: [EF4::ZERO; 4],
            phi_sum_next: [EF4::ZERO; 4],
        }
    }

    /// The running accumulator after constraint folding.
    pub(crate) fn accumulator(&self) -> EF4 {
        self.accumulator
    }

    /// Accumulate a RAP constraint using Horner's method.
    fn rap_assert_zero(&mut self, x: EF4) {
        self.accumulator = self.accumulator * self.alpha + x;
    }

    /// Generate cumsum constraints (call after chip.eval() returns).
    ///
    /// Delegates to [`cumsum_constraint_values`] for the 12 constraint
    /// expressions (shared with prover), then feeds each to `rap_assert_zero`.
    pub fn finalize_cumsum(&mut self, cumsum_final: [EF4; 4]) {
        let trace = self.full_trace;
        let local_row = trace.row_slice(0).expect("row exists");
        let next_row = trace.row_slice(1).expect("row exists");

        let cumsum_offset = self.main_width + self.interaction_index * 4;
        let cumsum_local = read_ef4_components(&local_row, cumsum_offset);
        let cumsum_next = read_ef4_components(&next_row, cumsum_offset);

        let sels = RowSelectors {
            is_first_row: self.is_first_row,
            is_last_row: self.is_last_row,
            is_transition: self.is_transition,
        };
        let constraints = cumsum_constraint_values(
            cumsum_local,
            cumsum_next,
            self.phi_sum_local,
            self.phi_sum_next,
            cumsum_final,
            sels,
        );
        for c in constraints {
            self.rap_assert_zero(c);
        }
    }

    /// Process a single interaction (shared logic for send/receive).
    fn process_interaction(&mut self, interaction: &AirInteraction<EF4>, is_send: bool) {
        let trace = self.full_trace;
        let local_row = trace.row_slice(0).expect("row exists");
        let next_row = trace.row_slice(1).expect("row exists");

        let phi_offset = self.main_width + self.interaction_index * 4;
        let phi_local = read_ef4_components(&local_row, phi_offset);

        // Compute fingerprint via shared helper.
        let [alpha, beta] = self.challenges;
        let tag = EF4::from(BabyBear::from_u64(interaction.bus.tag() as u64));
        let f = compute_fingerprint_components(
            ef4_coeffs(alpha),
            ef4_coeffs(beta),
            tag,
            &interaction.values,
        );

        // Constrain: phi · f = m (4 component equations).
        let product = ef4_mul(&phi_local, &f);
        let m = interaction.multiplicity;
        self.rap_assert_zero(product[0] - m);
        self.rap_assert_zero(product[1]);
        self.rap_assert_zero(product[2]);
        self.rap_assert_zero(product[3]);

        // Accumulate ±phi.
        let phi_next = read_ef4_components(&next_row, phi_offset);
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

/// Read 4 consecutive values from a row slice as an EF4-component array.
fn read_ef4_components<T: Copy>(row: &[T], offset: usize) -> [T; 4] {
    [row[offset], row[offset + 1], row[offset + 2], row[offset + 3]]
}

type VF4View<'a> =
    p3_matrix::stack::VerticalPair<RowMajorMatrixView<'a, EF4>, RowMajorMatrixView<'a, EF4>>;

impl<'a> AirBuilder for RapVerifierFolder<'a> {
    type F = BabyBear;
    type Expr = EF4;
    type Var = EF4;
    type M = VF4View<'a>;

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

impl<'a> PairBuilder for RapVerifierFolder<'a> {
    fn preprocessed(&self) -> Self::M {
        self.preprocessed.unwrap_or_else(|| {
            p3_matrix::stack::VerticalPair::new(
                RowMajorMatrixView::new(&[], 0),
                RowMajorMatrixView::new(&[], 0),
            )
        })
    }
}

impl<'a> AirBuilderWithPublicValues for RapVerifierFolder<'a> {
    type PublicVar = BabyBear;

    fn public_values(&self) -> &[Self::PublicVar] {
        self.public_values
    }
}

impl<'a> InteractionAirBuilder for RapVerifierFolder<'a> {
    fn send(&mut self, interaction: AirInteraction<Self::Expr>) {
        self.process_interaction(&interaction, true);
    }

    fn receive(&mut self, interaction: AirInteraction<Self::Expr>) {
        self.process_interaction(&interaction, false);
    }
}
