//! SSMC commitment scheme — bundles MemoryShard + StateShard + MetaShard.
//!
//! Implements [`ColumnCommitment`] by producing three shard chips per SSMC column:
//! - [`MemoryShardChip<W>`](super::memory::MemoryShardChip) — inter-tx ordering
//! - [`StateShardChip<W>`](super::state::StateShardChip) — old/new hash chains
//! - [`MetaShardChip`](super::meta::MetaShardChip) — commitment metadata
//!
//! ChipIds are allocated sequentially via [`ChipIdAllocator`] at construction time.

use std::collections::BTreeMap;

use tabula_commitment::scheme_tags;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use tabula_stark::air::interaction::{BusId, core_buses};
use tabula_stark::chips::{ChipId, ChipIdAllocator};
use tabula_stark::trace::column_commitment::{ColumnCommitment, ColumnPlan};
use tabula_stark::trace::contributor::WitnessStore;
use tabula_stark::trace::trace_map::TraceEntry;

use super::memory::trace::{MemoryShardRow, generate_memory_shard_trace};
use super::meta::trace::{MetaShardRow, generate_meta_shard_trace};
use super::state::trace::{StateShardRow, generate_state_shard_trace};

/// Per-column witness data for the SSMC commitment scheme.
///
/// Stored in [`WitnessStore`] under [`SSMC_WITNESS_LABEL`].
#[derive(Debug, Default)]
pub struct SsmcWitness {
    columns: BTreeMap<(TableId, ColId), SsmcColumnWitness>,
}

impl SsmcWitness {
    /// Insert witness data for a column.
    pub fn insert(&mut self, table: TableId, col: ColId, data: SsmcColumnWitness) {
        self.columns.insert((table, col), data);
    }

    /// Get witness data for a column.
    pub fn get(&self, table: TableId, col: ColId) -> Option<&SsmcColumnWitness> {
        self.columns.get(&(table, col))
    }

    /// Consume the witness and return the per-column data.
    pub fn take_columns(self) -> BTreeMap<(TableId, ColId), SsmcColumnWitness> {
        self.columns
    }
}

/// Witness data for a single SSMC column.
#[derive(Debug, Clone)]
pub struct SsmcColumnWitness {
    /// Memory shard rows (sorted by key, tx_index).
    pub memory_rows: Vec<MemoryShardRow>,
    /// State shard rows (sorted by key).
    pub state_rows: Vec<StateShardRow>,
    /// Meta shard row (None if column is unused).
    pub meta_row: Option<MetaShardRow>,
}

/// WitnessStore label for SSMC witness data.
pub const SSMC_WITNESS_LABEL: &str = "ssmc_witness";

/// Chip IDs allocated for a single SSMC column.
#[derive(Debug, Clone, Copy)]
struct SsmcColumnChips {
    memory: ChipId,
    state: ChipId,
    meta: ChipId,
}

/// SSMC commitment scheme: Sorted State Merkle Commitment.
///
/// Bundles three per-column shard chips into a single `ColumnCommitment`
/// implementation. ChipIds are allocated sequentially from a shared allocator.
///
/// Generic over `W` (value encoding width): 1 (Bool), 3 (U64/I64), 8 (Digest).
pub struct SsmcCommitment<const W: usize> {
    /// Maps (table, col) → allocated chip IDs.
    column_chips: BTreeMap<(TableId, ColId), SsmcColumnChips>,
}

impl<const W: usize> SsmcCommitment<W> {
    /// Create an SSMC commitment scheme from a proof plan.
    ///
    /// Only columns with `scheme_name == "ssmc"` matching this width class
    /// are included. Chip IDs are allocated sequentially from `alloc`.
    pub fn new(columns: &[ColumnPlan], alloc: &mut ChipIdAllocator) -> Self {
        let column_chips = columns
            .iter()
            .filter(|c| c.scheme_name == "ssmc")
            .map(|c| {
                let chips = SsmcColumnChips {
                    memory: alloc.next(),
                    state: alloc.next(),
                    meta: alloc.next(),
                };
                ((c.table, c.col), chips)
            })
            .collect();
        Self { column_chips }
    }
}

impl<const W: usize> ColumnCommitment for SsmcCommitment<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn chip_ids(&self) -> Vec<ChipId> {
        self.column_chips
            .values()
            .flat_map(|c| [c.memory, c.state, c.meta])
            .collect()
    }

    fn build_traces(
        &self,
        cols: &[ColumnPlan],
        store: &WitnessStore,
    ) -> Result<Vec<(ChipId, TraceEntry)>, TabulaError> {
        let witness = store.get::<SsmcWitness>(SSMC_WITNESS_LABEL)?;
        let mut entries = Vec::new();

        for col in cols {
            let chips = self
                .column_chips
                .get(&(col.table, col.col))
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "ssmc_build_traces",
                    detail: format!("no column index for ({}, {})", col.table.0, col.col.0),
                })?;

            let col_data =
                witness
                    .get(col.table, col.col)
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "ssmc_build_traces",
                        detail: format!(
                            "no SSMC witness data for ({}, {})",
                            col.table.0, col.col.0
                        ),
                    })?;

            let t = col.table.0;
            let c = col.col.0;

            let mem_trace = generate_memory_shard_trace::<W>(t, c, &col_data.memory_rows);
            let state_trace = generate_state_shard_trace::<W>(t, c, &col_data.state_rows);
            let meta_trace =
                generate_meta_shard_trace(t, c, scheme_tags::SSMC, col_data.meta_row.as_ref());

            entries.push((chips.memory, TraceEntry::main_only(mem_trace)));
            entries.push((chips.state, TraceEntry::main_only(state_trace)));
            entries.push((chips.meta, TraceEntry::main_only(meta_trace)));
        }

        Ok(entries)
    }

    fn output_buses(&self) -> Vec<BusId> {
        vec![
            core_buses::COMMITMENT_VERIF,
            core_buses::POSEIDON_PERM,
            core_buses::RANGE_CHECK,
            core_buses::BASE_STATE_ENTRY,
            core_buses::COALESCED_WRITE,
            core_buses::SMT_LEAF_DIGEST,
            core_buses::EMPTY_COL_READ,
        ]
    }
}
