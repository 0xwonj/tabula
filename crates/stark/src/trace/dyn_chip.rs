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
pub trait DynChip:
    ChipSpec
    + TraceContributor
    + BaseAir<BabyBear>
    + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>
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
