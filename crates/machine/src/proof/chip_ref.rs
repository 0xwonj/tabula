//! Sized adapter for `&dyn AnyRap` to satisfy Plonky3's `A: Sized` requirement.
//!
//! [`ChipRef`] wraps a borrowed trait object with optional preprocessed trace
//! data, implementing all `BaseAir` and `Air<AB>` bounds via delegation. This lets us pass dynamic chips to p3 functions.

use p3_air::{Air, BaseAir};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder};

use crate::AnyRap;
use crate::config::TabulaStarkConfig;
use tabula_stark::rap::prover::RapProverFolder;
use tabula_stark::rap::verifier::RapVerifierFolder;

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
    preprocessed: Option<RowMajorMatrix<KoalaBear>>,
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
    pub fn with_preprocessed(mut self, trace: RowMajorMatrix<KoalaBear>) -> Self {
        self.preprocessed = Some(trace);
        self
    }

    /// The underlying trait object.
    pub fn air(&self) -> &dyn AnyRap {
        self.air
    }
}

// ── BaseAir delegation ─────────────────────────────────────────────────────

impl BaseAir<KoalaBear> for ChipRef<'_> {
    fn width(&self) -> usize {
        <dyn AnyRap as BaseAir<KoalaBear>>::width(self.air)
    }

    fn preprocessed_trace(&self) -> Option<RowMajorMatrix<KoalaBear>> {
        self.preprocessed.clone()
    }

    fn num_public_values(&self) -> usize {
        <dyn AnyRap as BaseAir<KoalaBear>>::num_public_values(self.air)
    }
}

// ── Air<AB> delegation for each builder type ────────────────────────────────

impl Air<SymbolicAirBuilder<KoalaBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut SymbolicAirBuilder<KoalaBear>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<ProverConstraintFolder<'b, TabulaStarkConfig>> for ChipRef<'_> {
    fn eval(&self, builder: &mut ProverConstraintFolder<'b, TabulaStarkConfig>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<p3_uni_stark::DebugConstraintBuilder<'b, KoalaBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut p3_uni_stark::DebugConstraintBuilder<'b, KoalaBear>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<VerifierConstraintFolder<'b, TabulaStarkConfig>> for ChipRef<'_> {
    fn eval(&self, builder: &mut VerifierConstraintFolder<'b, TabulaStarkConfig>) {
        self.air.eval(builder);
    }
}

impl<'b> Air<tabula_stark::debug::DebugConstraintBuilder<'b, KoalaBear>> for ChipRef<'_> {
    fn eval(&self, builder: &mut tabula_stark::debug::DebugConstraintBuilder<'b, KoalaBear>) {
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
