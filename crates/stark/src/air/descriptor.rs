//! Per-chip interaction metadata extracted at keygen time.
//!
//! [`InteractionDescriptor`] captures what interactions a chip declares
//! as column references, enabling permutation trace generation without
//! re-evaluating `eval()`.

use p3_field::Field;

use super::interaction::Interaction;

/// Per-chip interaction metadata extracted at keygen time.
///
/// Describes what interactions a chip declares (as column references),
/// enabling permutation trace generation without re-evaluating `eval()`.
#[derive(Clone, Debug)]
pub struct InteractionDescriptor<F: Field> {
    /// Interactions this chip sends into LogUp buses.
    pub sends: Vec<Interaction<F>>,
    /// Interactions this chip receives from LogUp buses.
    pub receives: Vec<Interaction<F>>,
    /// Number of send interactions per row.
    pub num_sends_per_row: usize,
    /// Number of receive interactions per row.
    pub num_receives_per_row: usize,
}
