//! Sized adapter for `&dyn AnyRap` to satisfy Plonky3's `A: Sized` requirement.
//!
//! [`ChipRef`] wraps a borrowed trait object with optional preprocessed trace
//! data, implementing all `BaseAir`, `BaseAirWithPublicValues`, and `Air<AB>`
//! bounds via delegation. This lets us pass dynamic chips to p3 functions.

use p3_air::{Air, BaseAir, BaseAirWithPublicValues};
use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder};

use crate::AnyRap;
use crate::config::TabulaStarkConfig;
use crate::prove::RapProverFolder;
use crate::verify::RapVerifierFolder;

/// A `Sized` wrapper around `&dyn AnyRap` for passing to Plonky3 functions.
///
/// Holds an optional preprocessed trace (prover-side only), similar to the
/// the former compile-time `ChipInstance` wrapper.
///
/// # Why this exists
///
/// Plonky3's `prove()` and `verify()` require `A: Sized`. Trait objects
/// (`dyn AnyRap`) are `!Sized`, so we need this newtype wrapper to bridge
/// dynamic dispatch with p3's generic APIs.
pub struct ChipRef<'a> {
    air: &'a dyn AnyRap,
    preprocessed: Option<RowMajorMatrix<BabyBear>>,
}

impl<'a> ChipRef<'a> {
    /// Create a new chip reference from a trait object.
    pub fn new(air: &'a dyn AnyRap) -> Self {
        Self {
            air,
            preprocessed: None,
        }
    }

    /// Attach preprocessed trace data (for the prover).
    pub fn with_preprocessed(mut self, trace: RowMajorMatrix<BabyBear>) -> Self {
        self.preprocessed = Some(trace);
        self
    }

    /// The underlying trait object.
    pub fn air(&self) -> &dyn AnyRap {
        self.air
    }
}

// ── BaseAir delegation ─────────────────────────────────────────────────────

impl BaseAir<BabyBear> for ChipRef<'_> {
    fn width(&self) -> usize {
        <dyn AnyRap as BaseAir<BabyBear>>::width(self.air)
    }

    fn preprocessed_trace(&self) -> Option<RowMajorMatrix<BabyBear>> {
        self.preprocessed.clone()
    }
}

impl BaseAirWithPublicValues<BabyBear> for ChipRef<'_> {
    fn num_public_values(&self) -> usize {
        self.air.num_public_values()
    }
}

// ── Air<AB> delegation for each builder type ────────────────────────────────

impl Air<SymbolicAirBuilder<BabyBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut SymbolicAirBuilder<BabyBear>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<ProverConstraintFolder<'b, TabulaStarkConfig>> for ChipRef<'_> {
    fn eval(&self, builder: &mut ProverConstraintFolder<'b, TabulaStarkConfig>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<p3_uni_stark::DebugConstraintBuilder<'b, BabyBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut p3_uni_stark::DebugConstraintBuilder<'b, BabyBear>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<VerifierConstraintFolder<'b, TabulaStarkConfig>> for ChipRef<'_> {
    fn eval(&self, builder: &mut VerifierConstraintFolder<'b, TabulaStarkConfig>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<tabula_stark::debug::DebugConstraintBuilder<'b, BabyBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut tabula_stark::debug::DebugConstraintBuilder<'b, BabyBear>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<RapProverFolder<'b>> for ChipRef<'_> {
    fn eval(&self, builder: &mut RapProverFolder<'b>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<RapVerifierFolder<'b>> for ChipRef<'_> {
    fn eval(&self, builder: &mut RapVerifierFolder<'b>) {
        self.air.eval(builder);
    }
}
