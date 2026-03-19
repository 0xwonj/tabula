use super::memory::trace::MemoryShardRow;
use super::meta::trace::MetaShardRow;

/// Common per-column witness consumed by shared shard chips.
///
/// Each column-tier witness store carries exactly one of these under
/// [`SHARED_COLUMN_WITNESS_LABEL`], regardless of the commitment scheme.
#[derive(Debug, Clone, Default)]
pub struct SharedColumnWitness {
    /// Memory shard rows for this column.
    pub memory_rows: Vec<MemoryShardRow>,
    /// Meta shard row for this column, or `None` for a trivial trace.
    pub meta_row: Option<MetaShardRow>,
}

/// WitnessStore label for [`SharedColumnWitness`].
pub const SHARED_COLUMN_WITNESS_LABEL: &str = "shared_column_witness";
