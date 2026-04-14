//! Stable machine proving input types.

use tabula_core::{ColId, TableId};
use tabula_stark::air::statement::PublicStatement;
use tabula_stark::trace::WitnessStore;

pub(crate) mod assembly;

/// Stable identifier for a machine-managed column proof slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnSlotKey {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
}

impl std::fmt::Display for ColumnSlotKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.table.0, self.col.0)
    }
}

/// Prepared witness store for one machine-managed column proof slot.
pub struct PreparedColumnInput {
    /// Stable machine slot key.
    pub key: ColumnSlotKey,
    /// Witness store consumed to build this slot's traces.
    pub store: WitnessStore,
}

/// Prepared witness store for one machine-managed proof tier.
pub struct PreparedTierInput {
    /// Witness store consumed to build this tier's traces.
    pub store: WitnessStore,
}

/// Canonical prepared input bundle for machine proving.
pub struct PreparedMachineInput {
    /// Prepared execution-tier witness store.
    pub execution: PreparedTierInput,
    /// Prepared per-column witness stores, in proof-plan order.
    pub columns: Vec<PreparedColumnInput>,
    /// Prepared root-tier witness store.
    pub root: PreparedTierInput,
    /// AIR-level public values.
    pub public_statement: PublicStatement,
    /// Digest of the artifact-bound public statement bound into the transcript.
    pub binding_digest: [u8; 32],
}
