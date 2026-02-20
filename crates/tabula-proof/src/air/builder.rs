//! InteractionAirBuilder: trait extension for declaring LogUp interactions.
//!
//! Chips call `builder.send()` / `builder.receive()` inside their `eval()`
//! method to declare cross-chip LogUp interactions alongside local constraints.
//!
//! Different builder implementations handle these declarations differently:
//! - [`super::debug::DebugConstraintBuilder`]: records concrete values for LogUp balance checking
//! - Future `PermutationBuilder`: extracts symbolic descriptors for permutation trace generation

use p3_air::{AirBuilder, PairBuilder};

use super::interaction::AirInteraction;

/// Extension to [`AirBuilder`] for declaring LogUp interactions.
///
/// Chips implement `Air<AB>` where `AB: InteractionAirBuilder` to declare
/// both local constraints and cross-chip interactions in a single `eval()`.
///
/// The builder is responsible for interpreting these declarations:
/// collecting them for later verification, extracting static descriptors,
/// or generating permutation trace entries.
///
/// Extends [`PairBuilder`] to give all chips access to preprocessed columns
/// (e.g. PoseidonChip round constants). Chips that don't use preprocessed
/// columns simply ignore the `preprocessed()` method.
pub trait InteractionAirBuilder: AirBuilder + PairBuilder {
    /// Declare a send interaction (positive contribution to LogUp sum).
    ///
    /// The chip asserts that the tuple `(values, multiplicity)` appears
    /// in the bus identified by `interaction.kind`.
    fn send(&mut self, interaction: AirInteraction<Self::Expr>);

    /// Declare a receive interaction (negative contribution to LogUp sum).
    ///
    /// The chip asserts that it consumes a matching tuple from the bus.
    fn receive(&mut self, interaction: AirInteraction<Self::Expr>);
}
