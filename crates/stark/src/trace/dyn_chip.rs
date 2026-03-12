//! Object-safe chip trait for dynamic dispatch in trace building and validation.
//!
//! [`DynChip`] bundles the trait bounds needed by the witness crate's trace
//! orchestration and validation pipelines into a single object-safe supertrait.
//! A blanket impl covers every chip that satisfies the bounds.

use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;

use crate::chips::ChipSpec;
use crate::debug::DebugConstraintBuilder;
use crate::trace::contributor::TraceContributor;

/// Object-safe chip trait for trace building and debug validation.
///
/// Combines:
/// - [`ChipSpec`] — chip identity and metadata
/// - [`TraceContributor`] — phase-ordered trace generation
/// - [`BaseAir<BabyBear>`] — trace width queries
/// - [`Air<DebugConstraintBuilder>`] — constraint evaluation for interaction recording
///
/// Used as `&dyn DynChip` or `Box<dyn DynChip>` to iterate over heterogeneous
/// chip collections without compile-time enum dispatch.
///
/// # WitnessStore contract
///
/// Each chip's [`TraceContributor::contribute()`] reads specific entries from a
/// [`WitnessStore`]. The required labels are defined in
/// [`witness_labels`](crate::trace::witness_labels):
///
/// | Phase | Chip | Required Label(s) |
/// |-------|------|-------------------|
/// | Independent | ExecutionChip | `EXECUTION_RECORDS` |
/// | Independent | StaticTableChip | `STATIC_TABLE_ROWS` |
/// | Independent | SmtColPathChip | `SMT_COL_PATHS` |
/// | Independent | SmtTablePathChip | `SMT_TABLE_PATHS`, `SMT_TABLE_PVS` |
/// | Memory | MemoryShardChip | `SSMC_WITNESS_LABEL` (per-column) |
/// | Memory | StateShardChip | `SSMC_WITNESS_LABEL` (per-column) |
/// | Memory | MetaShardChip | `SSMC_WITNESS_LABEL` (per-column) |
/// | Memory | PropertyVerifierChip | `PROPERTY_READ_RECORDS` (per-column) |
/// | Dependent | PoseidonChip | `POSEIDON_INPUTS` (populated by `BusConsumer`) |
/// | Dependent | RangeCheckChip | `RANGE_CHECK_MULTS` (populated by `BusConsumer`) |
///
/// Labels for Dependent-phase chips are populated automatically by the
/// orchestrator's [`BusConsumer::collect()`] step between Phase 1 and Phase 2.
pub trait DynChip:
    ChipSpec + TraceContributor + BaseAir<BabyBear> + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>
{
}

/// Blanket impl: any type satisfying the bounds is automatically a [`DynChip`].
impl<T> DynChip for T where
    T: ChipSpec
        + TraceContributor
        + BaseAir<BabyBear>
        + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>
{
}
