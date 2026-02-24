//! Unified chip wrapper for proving and verification.
//!
//! [`ChipInstance<CS>`] wraps a chip set variant with:
//! - Optional preprocessed trace data (for the prover)
//! - Public values
//!
//! Replaces the separate `ProverAir<CS>` and `VerifierAir<CS>` wrappers
//! from the STARK module.

use p3_air::{Air, BaseAir, BaseAirWithPublicValues};
use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;

use super::builder::InteractionAirBuilder;
use super::chip_set::ChipSet;

/// A fully-instantiated chip ready for proving or verification.
///
/// Wraps a chip set variant with optional preprocessed trace data.
/// The `Air<AB>` impl delegates directly to the inner chip.
///
/// # Prover vs. Verifier
///
/// - **Prover**: Constructed with preprocessed trace data via `with_preprocessed()`.
///   The `BaseAir::preprocessed_trace()` method returns this data.
/// - **Verifier**: No preprocessed data needed — p3's `verify_with_preprocessed()`
///   receives the preprocessed verifier key from the proof.
pub struct ChipInstance<CS> {
    /// The underlying AIR chip (a ChipSet variant).
    air: CS,
    /// Optional preprocessed trace data (prover-side only).
    preprocessed: Option<RowMajorMatrix<BabyBear>>,
}

impl<CS: ChipSet> ChipInstance<CS> {
    /// Construct a chip instance from a chip set variant.
    pub fn new(air: CS) -> Self {
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

    /// The underlying chip set variant.
    pub fn air(&self) -> &CS {
        &self.air
    }

    /// Chip name (delegated to inner).
    pub fn name(&self) -> &'static str {
        self.air.chip_name()
    }

    /// Whether this chip has preprocessed data attached.
    pub fn has_preprocessed(&self) -> bool {
        self.preprocessed.is_some()
    }

    /// Number of public values for this chip.
    pub fn num_public_values(&self) -> usize {
        self.air.num_public_values()
    }

    /// Build chip instances from `CS::all_chips()` with trace data from a `TraceMap`.
    pub fn build_all(traces: &crate::trace_builder::TraceMap) -> Vec<Self> {
        CS::all_chips()
            .into_iter()
            .filter_map(|chip| {
                let name = chip.chip_name();
                let entry = traces.get(name)?;
                let mut instance = Self::new(chip);
                if let Some(pp) = &entry.preprocessed {
                    instance = instance.with_preprocessed(pp.clone());
                }
                Some(instance)
            })
            .collect()
    }
}

impl<CS: ChipSet> BaseAir<BabyBear> for ChipInstance<CS> {
    fn width(&self) -> usize {
        self.air.width()
    }

    fn preprocessed_trace(&self) -> Option<RowMajorMatrix<BabyBear>> {
        self.preprocessed.clone()
    }
}

impl<CS: ChipSet> BaseAirWithPublicValues<BabyBear> for ChipInstance<CS> {
    fn num_public_values(&self) -> usize {
        self.air.num_public_values()
    }
}

impl<CS, AB> Air<AB> for ChipInstance<CS>
where
    CS: ChipSet + Air<AB>,
    AB: InteractionAirBuilder<F = BabyBear> + p3_air::AirBuilderWithPublicValues,
{
    fn eval(&self, builder: &mut AB) {
        self.air.eval(builder);
    }
}
