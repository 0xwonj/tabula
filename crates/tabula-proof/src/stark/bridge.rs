//! Bridge implementations of [`EmptyMessageBuilder`] for p3-uni-stark builder types.
//!
//! Each p3-uni-stark builder type only needs to evaluate main AIR constraints.
//! Interactions are handled separately by the LogUp permutation system.
//! The [`EmptyMessageBuilder`] marker trait triggers a blanket no-op impl
//! of [`InteractionAirBuilder`], so `send()`/`receive()` are silently discarded.

use p3_field::{ExtensionField, Field};
use p3_uni_stark::StarkGenericConfig;

use crate::air::builder::EmptyMessageBuilder;

impl<F: Field, EF: ExtensionField<F>> EmptyMessageBuilder
    for p3_uni_stark::SymbolicAirBuilder<F, EF>
{
}

impl<SC: StarkGenericConfig> EmptyMessageBuilder for p3_uni_stark::ProverConstraintFolder<'_, SC> {}

impl<SC: StarkGenericConfig> EmptyMessageBuilder
    for p3_uni_stark::VerifierConstraintFolder<'_, SC>
{
}

impl<F: Field> EmptyMessageBuilder for p3_uni_stark::DebugConstraintBuilder<'_, F> {}
