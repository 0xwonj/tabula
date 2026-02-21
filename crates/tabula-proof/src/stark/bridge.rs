//! Bridge implementations of [`InteractionAirBuilder`] for p3-uni-stark builder types.
//!
//! Our AIR chips implement `Air<AB> where AB: InteractionAirBuilder + AirBuilderWithPublicValues`.
//! The p3-uni-stark prover/verifier use their own builder types (`ProverConstraintFolder`,
//! `VerifierConstraintFolder`, `SymbolicAirBuilder`, `DebugConstraintBuilder`) which implement
//! `AirBuilder + PairBuilder + AirBuilderWithPublicValues` but NOT `InteractionAirBuilder`.
//!
//! Since `InteractionAirBuilder` is defined in our crate, we can implement it for these
//! foreign types with no-op `send()`/`receive()`. This is correct because:
//! - Interactions are handled by the LogUp cross-chip balance check, not by per-chip constraints
//! - The prover/verifier constraint folders only need to evaluate main AIR constraints
//! - Interactions are recorded separately via [`crate::air::debug`]

use p3_field::{ExtensionField, Field};
use p3_uni_stark::{
    DebugConstraintBuilder, ProverConstraintFolder, StarkGenericConfig, SymbolicAirBuilder,
    VerifierConstraintFolder,
};

use crate::air::builder::InteractionAirBuilder;
use crate::air::interaction::AirInteraction;

// ─── SymbolicAirBuilder (constraint degree inference) ────────────────────────

impl<F, EF> InteractionAirBuilder for SymbolicAirBuilder<F, EF>
where
    F: Field,
    EF: ExtensionField<F>,
{
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op: symbolic builder is only used for constraint degree analysis.
    }

    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op.
    }
}

// ─── ProverConstraintFolder (prover-side constraint accumulation) ────────────

impl<SC: StarkGenericConfig> InteractionAirBuilder for ProverConstraintFolder<'_, SC> {
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op: interactions are handled by the LogUp permutation trace,
        // not by per-chip constraint accumulation.
    }

    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op.
    }
}

// ─── VerifierConstraintFolder (verifier-side constraint checking) ────────────

impl<SC: StarkGenericConfig> InteractionAirBuilder for VerifierConstraintFolder<'_, SC> {
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op: verifier checks main constraints only; LogUp is verified
        // via cross-chip cumulative sum equality.
    }

    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op.
    }
}

// ─── p3-uni-stark's DebugConstraintBuilder (debug-mode constraint checking) ─

impl<F: Field> InteractionAirBuilder for DebugConstraintBuilder<'_, F> {
    fn send(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op: p3-uni-stark's debug builder only checks constraint satisfaction.
        // Our own DebugConstraintBuilder in air::debug records interactions separately.
    }

    fn receive(&mut self, _interaction: AirInteraction<Self::Expr>) {
        // No-op.
    }
}
