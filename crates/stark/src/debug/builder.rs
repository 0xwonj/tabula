//! `DebugConstraintBuilder`: evaluates constraints on concrete field values.

use p3_air::{AirBuilder, RowWindow};
use p3_field::Field;

use crate::air::builder::InteractionAirBuilder;
use crate::air::interaction::{AirInteraction, InteractionDirection};

use super::errors::ConstraintError;
use super::logup::RecordedInteraction;

/// AirBuilder that evaluates constraints on concrete field values
/// and records LogUp interactions.
///
/// `Expr = F`, `Var = F`: expressions evaluate directly to field elements.
/// Interactions are collected into `self.interactions` for later analysis.
pub struct DebugConstraintBuilder<'a, F: Field> {
    pub(super) row_index: usize,
    pub(super) main: RowWindow<'a, F>,
    pub(super) preprocessed: RowWindow<'a, F>,
    pub(super) is_first_row: F,
    pub(super) is_last_row: F,
    pub(super) is_transition: F,
    pub(super) constraint_index: usize,
    pub(super) first_failure: Option<ConstraintError>,
    /// Interactions recorded during this row's evaluation.
    pub(super) interactions: Vec<RecordedInteraction<F>>,
    /// Public values available to chips via `AirBuilder::public_values()`.
    pub(super) public_values: &'a [F],
}

impl<'a, F: Field> AirBuilder for DebugConstraintBuilder<'a, F> {
    type F = F;
    type Expr = F;
    type Var = F;
    type PreprocessedWindow = RowWindow<'a, F>;
    type MainWindow = RowWindow<'a, F>;
    type PublicVar = F;

    fn main(&self) -> Self::MainWindow {
        self.main
    }

    fn preprocessed(&self) -> &Self::PreprocessedWindow {
        &self.preprocessed
    }

    fn is_first_row(&self) -> Self::Expr {
        self.is_first_row
    }

    fn is_last_row(&self) -> Self::Expr {
        self.is_last_row
    }

    fn is_transition_window(&self, _size: usize) -> Self::Expr {
        self.is_transition
    }

    fn assert_zero<I: Into<Self::Expr>>(&mut self, x: I) {
        let val = x.into();
        if val != F::ZERO && self.first_failure.is_none() {
            self.first_failure = Some(ConstraintError {
                row: self.row_index,
                constraint_index: self.constraint_index,
                value: format!("{val:?}"),
            });
        }
        self.constraint_index += 1;
    }

    fn public_values(&self) -> &[Self::PublicVar] {
        self.public_values
    }
}

impl<'a, F: Field> InteractionAirBuilder for DebugConstraintBuilder<'a, F> {
    fn send(&mut self, interaction: AirInteraction<F>) {
        self.interactions.push(RecordedInteraction {
            bus: interaction.bus,
            values: interaction.values,
            multiplicity: interaction.multiplicity,
            direction: InteractionDirection::Send,
        });
    }

    fn receive(&mut self, interaction: AirInteraction<F>) {
        self.interactions.push(RecordedInteraction {
            bus: interaction.bus,
            values: interaction.values,
            multiplicity: interaction.multiplicity,
            direction: InteractionDirection::Receive,
        });
    }
}
