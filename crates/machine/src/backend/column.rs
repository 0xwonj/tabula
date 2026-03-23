//! Proof-only per-column seam consumed by the STARK machine.

use tabula_core::{ColId, SchemeId, TableId};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::{BusConsumer, DynChip};

use crate::backend::AnyRap;
use crate::setup::registry::SetupError;

/// Chips produced by one column scheme for a single column proof tier.
pub struct ColumnChipSet {
    /// AIR implementations for proving/verification.
    pub airs: Vec<Box<dyn AnyRap>>,
    /// Dynamic chips for phase-ordered trace generation.
    pub dyn_chips: Vec<Box<dyn DynChip>>,
    /// Optional scheme-owned dependent bus consumers.
    pub bus_consumers: Vec<Box<dyn BusConsumer>>,
}

/// Proving-facing per-column view.
pub trait ProofColumn: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Table identifier of this committed column.
    fn table_id(&self) -> TableId;

    /// Column identifier of this committed column.
    fn col_id(&self) -> ColId;

    /// Portable scheme identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Create shard chips for this column.
    fn create_chips(&self, alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError>;
}
