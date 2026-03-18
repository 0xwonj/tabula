//! Witness partitioning for multi-proof architectures.
//!
//! [`partition_by_tier()`] splits a global [`WitnessStore`] into per-tier
//! partitions (execution, per-column, root) for the C+2 sharded proof
//! architecture.
//!
//! The caller supplies the already-prepared per-column witness stores so that
//! built-in and custom schemes can follow the same prepared-column path.

use tabula_core::{ColId, TableId};

use tabula_stark::trace::{WitnessStore, witness_labels};

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
    /// Each store contains whatever labels the prepared scheme for that column
    /// contributed for its chips.
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
/// [`BuiltinTraceBuilder::prepare_witness_store()`]) plus per-column stores already
/// prepared by the selected column schemes, then produces the execution/root
/// partitions alongside those column stores.
pub fn partition_by_tier(
    mut global_store: WitnessStore,
    columns: Vec<((TableId, ColId), WitnessStore)>,
) -> PartitionedStores {
    PartitionedStores {
        execution: global_store.drain_labels(EXECUTION_LABELS),
        columns,
        root: global_store.drain_labels(ROOT_LABELS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_by_tier_creates_per_tier_stores() {
        let global = WitnessStore::new();
        let stores = partition_by_tier(global, Vec::new());
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

        let stores = partition_by_tier(global, Vec::new());

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
