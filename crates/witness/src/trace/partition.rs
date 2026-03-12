//! Witness partitioning for multi-proof architectures.
//!
//! [`partition_by_tier()`] splits a global [`WitnessStore`] into per-tier
//! partitions (execution, per-column, root) for the C+2 sharded proof
//! architecture.
//!
//! Partitioning is label-based: entries are drained from the global store by
//! their [`witness_labels`] key, without requiring knowledge of concrete chip
//! types. This keeps the witness layer decoupled from chip implementations.

use std::collections::BTreeMap;

use tabula_core::{ColId, TableId};

use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};

use tabula_stark::trace::{WitnessStore, witness_labels};

use super::builder::PROPERTY_READ_ALL_LABEL;

// ── Sharded partitioning ──────────────────────────────────────────────────

/// Per-tier witness stores for the proof architecture.
///
/// Contains one store per proof instance in the C+2 architecture:
/// - 1 execution store
/// - C column stores (one per `(table, col)`)
/// - 1 root store
pub struct PartitionedStores {
    /// Execution tier: instruction records + static table rows.
    pub execution: WitnessStore,
    /// Column tiers: per-(table, col) shard witness data.
    ///
    /// Each store contains a single-column [`SsmcWitness`] for its
    /// shard chips (MemoryShardChip, StateShardChip, MetaShardChip).
    pub columns: Vec<((TableId, ColId), WitnessStore)>,
    /// Root tier: column metadata + SMT paths.
    pub root: WitnessStore,
}

/// Labels belonging to the execution tier.
const EXECUTION_LABELS: &[&str] = &[
    witness_labels::EXECUTION_RECORDS,
    witness_labels::STATIC_TABLE_ROWS,
];

/// Labels belonging to the root tier.
const ROOT_LABELS: &[&str] = &[
    witness_labels::SMT_COL_PATHS,
    witness_labels::SMT_TABLE_PATHS,
    witness_labels::SMT_TABLE_PVS,
];

/// Split witness data into per-tier stores.
///
/// Takes a global [`WitnessStore`] (as populated by
/// [`TraceBuilder::prepare_witness_store()`]) plus per-column shard data
/// (from [`prepare_shard_witness()`]) and produces separate stores for
/// each proof tier.
///
/// Entries are **drained** (moved) from the global store by label, so
/// no concrete chip types need to be imported. Each tier's store is
/// self-contained for trace building.
///
/// [`prepare_shard_witness()`]: super::memory::prepare_shard_witness
/// [`TraceBuilder::prepare_witness_store()`]: super::builder::TraceBuilder::prepare_witness_store
pub fn partition_by_tier(
    mut global_store: WitnessStore,
    shard_witness: SsmcWitness,
) -> PartitionedStores {
    let exec_store = global_store.drain_labels(EXECUTION_LABELS);

    // Extract PropertyRead records (grouped by column) from the global store.
    let mut property_records: BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>> = global_store
        .get::<BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>>(PROPERTY_READ_ALL_LABEL)
        .cloned()
        .unwrap_or_default();

    let columns: Vec<((TableId, ColId), WitnessStore)> = shard_witness
        .take_columns()
        .into_iter()
        .map(|((table, col), col_data)| {
            let mut col_store = WitnessStore::new();
            let mut single_witness = SsmcWitness::default();
            single_witness.insert(table, col, col_data);
            col_store.put(SSMC_WITNESS_LABEL, single_witness);

            // Insert PropertyRead records for this column (if any).
            if let Some(records) = property_records.remove(&(table, col)) {
                col_store.put(PROPERTY_READ_WITNESS_LABEL, records);
            }

            ((table, col), col_store)
        })
        .collect();

    let root_store = global_store.drain_labels(ROOT_LABELS);

    PartitionedStores {
        execution: exec_store,
        columns,
        root: root_store,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_chips::shards::ssmc::SsmcWitness;

    #[test]
    fn partition_by_tier_creates_per_tier_stores() {
        let global = WitnessStore::new();
        let shard_witness = SsmcWitness::default();
        let stores = partition_by_tier(global, shard_witness);
        // Execution store exists (may be empty).
        assert!(stores.columns.is_empty());
        // Root store exists.
        let _ = &stores.root;
    }

    #[test]
    fn partition_drains_labels_to_correct_tiers() {
        let mut global = WitnessStore::new();
        global.put(witness_labels::EXECUTION_RECORDS, vec![1u32, 2, 3]);
        global.put(witness_labels::SMT_COL_PATHS, vec![10u32, 20]);

        let shard_witness = SsmcWitness::default();
        let stores = partition_by_tier(global, shard_witness);

        // Execution tier got its label.
        assert!(
            stores
                .execution
                .contains::<Vec<u32>>(witness_labels::EXECUTION_RECORDS)
        );
        // Root tier got its label.
        assert!(
            stores
                .root
                .contains::<Vec<u32>>(witness_labels::SMT_COL_PATHS)
        );
        // Cross-check: execution doesn't have root labels.
        assert!(
            !stores
                .execution
                .contains::<Vec<u32>>(witness_labels::SMT_COL_PATHS)
        );
    }
}
