//! No-op [`InteractionAirBuilder`] impls for p3-uni-stark builder types.
//!
//! Each p3-uni-stark builder type only needs to evaluate main AIR constraints.
//! Interactions are handled separately by the LogUp permutation system.
//! These no-op impls allow `send()`/`receive()` to be silently discarded
//! when evaluating chips with p3's own builders.

use p3_field::{ExtensionField, Field};
use p3_uni_stark::StarkGenericConfig;

use crate::air::builder::InteractionAirBuilder;
use crate::air::interaction::AirInteraction;

impl<F: Field, EF: ExtensionField<F>> InteractionAirBuilder
    for p3_uni_stark::SymbolicAirBuilder<F, EF>
{
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {}
    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {}
}

impl<SC: StarkGenericConfig> InteractionAirBuilder
    for p3_uni_stark::ProverConstraintFolder<'_, SC>
{
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {}
    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {}
}

impl<SC: StarkGenericConfig> InteractionAirBuilder
    for p3_uni_stark::VerifierConstraintFolder<'_, SC>
{
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {}
    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {}
}

impl<F: Field> InteractionAirBuilder for p3_uni_stark::DebugConstraintBuilder<'_, F> {
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {}
    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {}
}
