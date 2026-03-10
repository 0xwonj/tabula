//! Witness partitioning for multi-proof architectures.
//!
//! A [`WitnessPartition`] wraps a [`WitnessStore`] containing the subset
//! of witness data needed by one proof instance. In the current monolithic
//! prover, a single partition contains all data; future sharded provers
//! create per-tier partitions (execution, per-column, root).

use tabula_stark::trace::WitnessStore;

/// A partition of witness data for a single proof instance.
///
/// Thin wrapper around [`WitnessStore`], representing the witness data
/// subset for one [`ProofInstance`]. The current monolithic prover uses
/// a single partition with all data. Sharded provers (Goal 3) will create
/// multiple partitions via tier-based splitting.
///
/// [`ProofInstance`]: tabula_machine::ProofInstance
pub struct WitnessPartition {
    store: WitnessStore,
}

impl WitnessPartition {
    /// Create a partition from a [`WitnessStore`].
    pub fn from_store(store: WitnessStore) -> Self {
        Self { store }
    }

    /// Consume the partition, returning the underlying [`WitnessStore`].
    pub fn into_store(self) -> WitnessStore {
        self.store
    }

    /// Borrow the underlying store.
    pub fn store(&self) -> &WitnessStore {
        &self.store
    }
}

/// Create a single partition containing all witness data (no splitting).
///
/// This is the default partitioning strategy for the monolithic prover.
/// Equivalent to `WitnessPartition::from_store(store)`.
///
/// Future sharded provers will use tier-based partitioning that splits
/// the store by proof tier:
/// - **Execution**: `InstructionRecords`, `StaticTableRows`
/// - **Column [i]**: per-(t,c) memory accesses, SSMC witness, SMT paths
/// - **Root**: `ColumnMetaInput`, SMT table paths
pub fn single_partition(store: WitnessStore) -> WitnessPartition {
    WitnessPartition::from_store(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_round_trip() {
        let mut store = WitnessStore::new();
        store.put("test_data", vec![1u32, 2, 3]);
        store.put("other_data", 42u64);

        let partition = WitnessPartition::from_store(store);

        // Verify store is accessible through partition.
        assert!(partition.store().contains::<Vec<u32>>("test_data"));
        assert!(partition.store().contains::<u64>("other_data"));

        // Round-trip: partition → store → partition.
        let store = partition.into_store();
        assert!(store.get::<Vec<u32>>("test_data").is_ok());
        assert_eq!(*store.get::<u64>("other_data").unwrap(), 42);
    }

    #[test]
    fn single_partition_preserves_all_data() {
        let mut store = WitnessStore::new();
        store.put("alpha", vec![10u32, 20]);
        store.put("beta", String::from("hello"));

        let partition = single_partition(store);
        let store = partition.into_store();

        assert_eq!(store.get::<Vec<u32>>("alpha").unwrap(), &vec![10, 20]);
        assert_eq!(store.get::<String>("beta").unwrap(), "hello");
    }
}
