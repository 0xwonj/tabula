//! Dedicated proof lane for static canonical relation tables.
//!
//! The witness rows are the sealed relation rows derived from the registered
//! program. AIR binds execution membership sends to those rows and binds the
//! full static table to one public root.
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::ChipId;

/// Witness-store label consumed by [`RelationTableChip`].
pub const RELATION_TABLE_WITNESS_LABEL: &str = "relation_table_rows";
/// Chip id for the static relation lookup lane.
pub const RELATION_TABLE_CHIP_ID: ChipId = ChipId(92);
/// Private bus carrying `(relation_id, input_digest, output_digest)` tuples.
pub const RELATION_TABLE_BUS: BusId = BusId(101);

pub(super) const RELATION_TABLE_DOMAIN_TAG: u32 = 0x52;

/// One static relation lookup row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTableWitnessRow {
    /// Relation identifier.
    pub relation_id: u32,
    /// Canonical input digest.
    pub input_digest: [u32; 8],
    /// Canonical output digest.
    pub output_digest: [u32; 8],
    /// Multiplicity on the lookup bus.
    pub lookup_mult: u32,
}
