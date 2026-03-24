//! Sized adapter for `&dyn AnyRap` to satisfy Plonky3's `A: Sized` requirement.

use p3_air::{Air, BaseAir};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder};

use crate::backend::AnyRap;
use crate::config::TabulaStarkConfig;
use tabula_stark::rap::prover::RapProverFolder;
use tabula_stark::rap::verifier::RapVerifierFolder;

/// A `Sized` wrapper around `&dyn AnyRap` for passing to Plonky3 functions.
pub struct ChipRef<'a> {
    air: &'a dyn AnyRap,
}

impl<'a> ChipRef<'a> {
    /// Create a new chip reference from a trait object.
    pub fn new(air: &'a dyn AnyRap) -> Self {
        Self { air }
    }

    /// The underlying trait object.
    pub fn air(&self) -> &dyn AnyRap {
        self.air
    }
}

impl BaseAir<KoalaBear> for ChipRef<'_> {
    fn width(&self) -> usize {
        <dyn AnyRap as BaseAir<KoalaBear>>::width(self.air)
    }

    fn preprocessed_trace(&self) -> Option<RowMajorMatrix<KoalaBear>> {
        None
    }

    fn preprocessed_next_row_columns(&self) -> Vec<usize> {
        <dyn AnyRap as BaseAir<KoalaBear>>::preprocessed_next_row_columns(self.air)
    }

    fn num_public_values(&self) -> usize {
        <dyn AnyRap as BaseAir<KoalaBear>>::num_public_values(self.air)
    }
}

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
