use tabula_stark::trace::{WitnessStore, witness_labels};

/// Per-tier witness stores for the C+2 proof architecture.
pub(crate) struct PartitionedStores {
    pub(crate) execution: WitnessStore,
    pub(crate) root: WitnessStore,
}

const ROOT_LABELS: &[&str] = &[
    witness_labels::SMT_COL_PATHS,
    witness_labels::SMT_TABLE_PATHS,
    witness_labels::SMT_TABLE_PVS,
];

pub(crate) fn partition_by_tier(mut global_store: WitnessStore) -> PartitionedStores {
    let root = global_store.drain_labels(ROOT_LABELS);
    PartitionedStores {
        execution: global_store,
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_by_tier_creates_per_tier_stores() {
        let global = WitnessStore::new();
        let stores = partition_by_tier(global);
        let _ = &stores.root;
    }

    #[test]
    fn partition_drains_labels_to_correct_tiers() {
        let mut global = WitnessStore::new();
        global.put(witness_labels::EXECUTION_RECORDS, vec![1u32, 2, 3]);
        global.put(witness_labels::SMT_COL_PATHS, vec![10u32, 20]);
        global.put("custom_execution", vec![99u32]);

        let stores = partition_by_tier(global);

        assert!(
            stores
                .execution
                .contains::<Vec<u32>>(witness_labels::EXECUTION_RECORDS)
        );
        assert!(
            stores
                .root
                .contains::<Vec<u32>>(witness_labels::SMT_COL_PATHS)
        );
        assert!(
            !stores
                .execution
                .contains::<Vec<u32>>(witness_labels::SMT_COL_PATHS)
        );
        assert!(stores.execution.contains::<Vec<u32>>("custom_execution"));
    }
}
