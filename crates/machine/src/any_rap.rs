//! Type-erased AIR trait for runtime chip composition.
//!
//! [`AnyRap`] bundles [`ChipSpec`] + [`BaseAir`] + all concrete [`Air<AB>`]
//! bounds needed by the prover/verifier into a single object-safe trait.
//! A blanket impl covers every chip that satisfies the bounds, so chip
//! authors never implement `AnyRap` manually.
//!
//! ```ignore
//! let chip: Box<dyn AnyRap> = Box::new(ExecutionChip::<3>);
//! chip.eval(&mut symbolic_builder);  // vtable dispatch
//! ```

use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;
use p3_uni_stark::{ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder};

use tabula_stark::chips::ChipSpec;

use crate::config::TabulaStarkConfig;
use crate::prove::RapProverFolder;
use crate::verify::RapVerifierFolder;

/// Object-safe supertrait bundling all AIR bounds for the STARK pipeline.
///
/// The prover/verifier call `eval()` with seven different builder types:
///
/// | Builder | Purpose |
/// |---------|---------|
/// | `SymbolicAirBuilder` | Constraint degree inference |
/// | `ProverConstraintFolder` | Prover constraint accumulation |
/// | `p3 DebugConstraintBuilder` | p3's internal debug checking |
/// | `VerifierConstraintFolder` | Verifier constraint checking |
/// | `tabula DebugConstraintBuilder` | LogUp interaction recording |
/// | `RapProverFolder` | Phase 2 RAP prover |
/// | `RapVerifierFolder` | Phase 2 RAP verifier |
///
/// The blanket impl covers all chips automatically — no manual implementation needed.
pub trait AnyRap:
    ChipSpec
    + BaseAir<BabyBear>
    + Air<SymbolicAirBuilder<BabyBear>>
    + for<'a> Air<ProverConstraintFolder<'a, TabulaStarkConfig>>
    + for<'a> Air<p3_uni_stark::DebugConstraintBuilder<'a, BabyBear>>
    + for<'a> Air<VerifierConstraintFolder<'a, TabulaStarkConfig>>
    + for<'a> Air<tabula_stark::debug::DebugConstraintBuilder<'a, BabyBear>>
    + for<'a> Air<RapProverFolder<'a>>
    + for<'a> Air<RapVerifierFolder<'a>>
    + Send
    + Sync
{
}

impl<T> AnyRap for T where
    T: ChipSpec
        + BaseAir<BabyBear>
        + Air<SymbolicAirBuilder<BabyBear>>
        + for<'a> Air<ProverConstraintFolder<'a, TabulaStarkConfig>>
        + for<'a> Air<p3_uni_stark::DebugConstraintBuilder<'a, BabyBear>>
        + for<'a> Air<VerifierConstraintFolder<'a, TabulaStarkConfig>>
        + for<'a> Air<tabula_stark::debug::DebugConstraintBuilder<'a, BabyBear>>
        + for<'a> Air<RapProverFolder<'a>>
        + for<'a> Air<RapVerifierFolder<'a>>
        + Send
        + Sync
{
}
