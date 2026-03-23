//! SMT commitment scheme — bundles MemoryShard + MetaShard.
//!
//! Implements [`ColumnCommitment`] for columns using Sparse Merkle Tree commitments.
//! Each SMT column gets two shard chips:
//! - [`MemoryShardChip<W>`](super::memory::MemoryShardChip) — inter-tx ordering (same as SSMC)
//! - [`MetaShardChip`](super::meta::MetaShardChip) — commitment metadata + leaf digest
//!
//! Unlike SSMC, SMT does NOT have a StateShard for computing commitments.
//! The per-column SMT root is a witness input; root verification is delegated
//! to the global Layer 2 chips (`SmtColPathChip` + `SmtTablePathChip`) which
//! receive leaf digests via the C15 SmtLeafDigest bus.
//!
//! ChipIds are allocated sequentially via [`ChipIdAllocator`] at construction time.

use std::collections::BTreeMap;

use tabula_commitment::{PoseidonHasher, compute_column_root_binding_prefix_digest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use tabula_stark::air::interaction::{BusId, core_buses};
use tabula_stark::chips::{ChipId, ChipIdAllocator};
use tabula_stark::trace::column_commitment::{ColumnCommitment, ColumnPlan};
use tabula_stark::trace::contributor::WitnessStore;
use tabula_stark::trace::trace_map::TraceEntry;

use super::memory::trace::{MemoryShardRow, generate_memory_shard_trace};
use super::meta::trace::{MetaShardRow, generate_meta_shard_trace};

/// Per-column witness data for the SMT commitment scheme.
///
/// Stored in [`WitnessStore`] under [`SMT_WITNESS_LABEL`].
#[derive(Debug, Default)]
pub struct SmtWitness {
    columns: BTreeMap<(TableId, ColId), SmtColumnWitness>,
}

impl SmtWitness {
    /// Insert witness data for a column.
    pub fn insert(&mut self, table: TableId, col: ColId, data: SmtColumnWitness) {
        self.columns.insert((table, col), data);
    }

    /// Get witness data for a column.
    pub fn get(&self, table: TableId, col: ColId) -> Option<&SmtColumnWitness> {
        self.columns.get(&(table, col))
    }
}

/// Witness data for a single SMT column.
///
/// Similar to [`SsmcColumnWitness`](super::ssmc::SsmcColumnWitness) but without
/// state shard rows — SMT root verification is handled at Layer 2.
#[derive(Debug, Clone)]
pub struct SmtColumnWitness {
    /// Memory shard rows (sorted by key, tx_index).
    pub memory_rows: Vec<MemoryShardRow>,
    /// Meta shard row (None if column is unused).
    pub meta_row: Option<MetaShardRow>,
}

/// WitnessStore label for SMT witness data.
pub const SMT_WITNESS_LABEL: &str = "smt_witness";

/// Chip IDs allocated for a single SMT column.
#[derive(Debug, Clone, Copy)]
struct SmtColumnChips {
    memory: ChipId,
    meta: ChipId,
}

/// SMT commitment scheme: Sparse Merkle Tree.
///
/// Bundles two per-column shard chips into a single `ColumnCommitment`
/// implementation. ChipIds are allocated sequentially from a shared allocator.
///
/// Generic over `W` (value encoding width): 1 (Bool), 3 (U64/I64), 8 (Digest).
pub struct SmtCommitment<const W: usize> {
    /// Maps (table, col) → allocated chip IDs.
    column_chips: BTreeMap<(TableId, ColId), SmtColumnChips>,
}

impl<const W: usize> SmtCommitment<W> {
    /// Create an SMT commitment scheme from a proof plan.
    ///
    /// Only columns with `scheme_name == "smt"` are included.
    /// Chip IDs are allocated sequentially from `alloc`.
    pub fn new(columns: &[ColumnPlan], alloc: &mut ChipIdAllocator) -> Self {
        let column_chips = columns
            .iter()
            .filter(|c| c.scheme_name == "smt")
            .map(|c| {
                let chips = SmtColumnChips {
                    memory: alloc.next(),
                    meta: alloc.next(),
                };
                ((c.table, c.col), chips)
            })
            .collect();
        Self { column_chips }
    }
}

impl<const W: usize> ColumnCommitment for SmtCommitment<W> {
    fn name(&self) -> &str {
        "smt"
    }

    fn chip_ids(&self) -> Vec<ChipId> {
        self.column_chips
            .values()
            .flat_map(|c| [c.memory, c.meta])
            .collect()
    }

    fn build_traces(
        &self,
        cols: &[ColumnPlan],
        store: &WitnessStore,
    ) -> Result<Vec<(ChipId, TraceEntry)>, TabulaError> {
        let witness = store.get::<SmtWitness>(SMT_WITNESS_LABEL)?;
        let mut entries = Vec::new();
        let hasher = PoseidonHasher::new();

        for col in cols {
            let chips = self
                .column_chips
                .get(&(col.table, col.col))
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "smt_build_traces",
                    detail: format!("no column index for ({}, {})", col.table.0, col.col.0),
                })?;

            let col_data =
                witness
                    .get(col.table, col.col)
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "smt_build_traces",
                        detail: format!("no SMT witness data for ({}, {})", col.table.0, col.col.0),
                    })?;

            let t = col.table.0;
            let c = col.col.0;

            let mem_trace = generate_memory_shard_trace::<W>(t, c, &col_data.memory_rows);
            let meta_trace = generate_meta_shard_trace(
                t,
                c,
                compute_column_root_binding_prefix_digest(
                    &hasher,
                    col.table,
                    col.col,
                    col.root_binding_family,
                    &col.column_profile_hash,
                ),
                col_data.meta_row.as_ref(),
            );

            entries.push((chips.memory, TraceEntry::main_only(mem_trace)));
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
