//! Local state overlay for deterministic execution.
//!
//! The overlay owns only state semantics:
//! - **Read-your-writes**
//! - **Read deduplication**
//! - **Write coalescing**
//!
//! Typed execution effects are recorded by the executor journal layer, not by
//! the overlay itself.

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CellKey, TypeId};
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::execution_state::ExecutionState;
use crate::journal::{TypedStateSnapshot, TypedStateWrite};

/// Finalized overlay state output, consumed by the batch executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayResult {
    /// Cells read from committed state (deduplicated). `None` = absent.
    pub read_set_old: Vec<TypedStateSnapshot>,
    /// Final writes to committed state (coalesced). `None` = delete.
    pub write_set_final: Vec<TypedStateWrite>,
}

/// A local overlay sitting on top of a `StateView`.
///
/// All reads go through the overlay; writes are buffered locally.
/// Supports checkpoint/rollback for per-tx failure recovery.
///
/// Uses an undo-log for O(1) checkpoint and O(k) rollback (where k
/// is the number of mutations since the checkpoint).
pub struct Overlay<'a, S: StateView> {
    state: ExecutionState<'a, S>,
    type_runtimes: &'a TypeRuntimeRegistry,
}

impl<'a, S: StateView> Overlay<'a, S> {
    /// Create a new overlay on top of a snapshot.
    pub fn new(snapshot: &'a S, type_runtimes: &'a TypeRuntimeRegistry) -> Self {
        Self {
            state: ExecutionState::new(snapshot),
            type_runtimes,
        }
    }

    /// Read a cell: checks write buffer, then read cache, then snapshot.
    pub fn read(
        &mut self,
        key: &CellKey,
        col_type: TypeId,
    ) -> Result<Option<TypedValue>, TabulaError> {
        if let Some(entry) = self.state.read_from_buffer(key) {
            return Ok(entry.value.clone());
        }
        if let Some(entry) = self.state.read_from_cache(key) {
            return Ok(entry.value.clone());
        }
        Ok(self
            .state
            .read_from_snapshot(key, col_type, self.type_runtimes)?
            .value)
    }

    /// Buffer a write to a cell.
    pub fn write(
        &mut self,
        key: &CellKey,
        value: Option<TypedValue>,
        col_type: TypeId,
    ) -> Result<(), TabulaError> {
        self.state.write_buffered(key, col_type, value);
        Ok(())
    }

    /// Save the current overlay state for potential rollback. O(1).
    pub fn checkpoint(&mut self) {
        self.state.checkpoint();
    }

    /// Restore the overlay to the most recent checkpoint. O(k).
    pub fn rollback(&mut self) -> Option<()> {
        self.state.rollback()
    }

    /// Discard the most recent checkpoint.
    pub fn discard_checkpoint(&mut self) {
        self.state.discard_checkpoint();
    }

    /// Finalize the overlay into typed state results.
    pub fn into_result(self) -> Result<OverlayResult, TabulaError> {
        let (read_set_old, write_set_final) = self.state.into_sets(self.type_runtimes)?;
        Ok(OverlayResult {
            read_set_old,
            write_set_final,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU32, Ordering};

    use tabula_core::error::TabulaError;
    use tabula_core::traits::StateView;
    use tabula_core::{CellKey, ColId, PortableValue, RowKey, TableId};
    use tabula_profile::TYPE_U64_ID;
    use tabula_types::{TypeRuntimeRegistry, bool_portable, u64_portable, u64_typed};

    use crate::execution_state::ExecutionState;

    const TY: tabula_core::TypeId = TYPE_U64_ID;

    fn type_runtimes() -> &'static TypeRuntimeRegistry {
        static TYPE_RUNTIMES: OnceLock<TypeRuntimeRegistry> = OnceLock::new();
        TYPE_RUNTIMES.get_or_init(|| TypeRuntimeRegistry::seeded().expect("seeded type runtimes"))
    }

    fn cell(t: u32, r: u64, c: u16) -> CellKey {
        CellKey {
            table: TableId(t),
            col: ColId(c),
            row: RowKey(r),
        }
    }

    struct CountingSnapshot {
        data: BTreeMap<CellKey, PortableValue>,
        call_count: AtomicU32,
    }

    impl CountingSnapshot {
        fn new(data: BTreeMap<CellKey, u64>) -> Self {
            Self {
                data: data
                    .into_iter()
                    .map(|(key, value)| (key, u64_portable(value)))
                    .collect(),
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    impl StateView for CountingSnapshot {
        fn read(&self, key: &CellKey) -> Result<Option<PortableValue>, TabulaError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(self.data.get(key).cloned())
        }

        fn table_exists(&self, _: TableId) -> bool {
            true
        }
    }

    #[test]
    fn execution_state_buffer_roundtrip() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut state = ExecutionState::new(&snap);
        let k = cell(1, 0, 0);

        assert!(state.read_from_buffer(&k).is_none());
        state.write_buffered(&k, TY, Some(u64_typed(42)));
        assert_eq!(
            state.read_from_buffer(&k).map(|entry| entry.value.clone()),
            Some(Some(u64_typed(42)))
        );
    }

    #[test]
    fn execution_state_cache_and_snapshot() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, 100);
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        assert!(state.read_from_cache(&k).is_none());
        let v = state.read_from_snapshot(&k, TY, type_runtimes()).unwrap();
        assert_eq!(v.value, Some(u64_typed(100)));
        assert_eq!(
            state.read_from_cache(&k).map(|entry| entry.value.clone()),
            Some(Some(u64_typed(100)))
        );
        assert_eq!(snap.call_count(), 1);
    }

    #[test]
    fn execution_state_rollback_restores_buffer() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut state = ExecutionState::new(&snap);
        let k = cell(1, 0, 0);

        state.write_buffered(&k, TY, Some(u64_typed(10)));
        state.checkpoint();
        state.write_buffered(&k, TY, Some(u64_typed(20)));
        state.rollback();

        assert_eq!(
            state.read_from_buffer(&k).map(|entry| entry.value.clone()),
            Some(Some(u64_typed(10)))
        );
    }

    #[test]
    fn execution_state_rollback_clears_new_cache() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, 100);
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        state.checkpoint();
        let _ = state.read_from_snapshot(&k, TY, type_runtimes()).unwrap();
        assert!(state.read_from_cache(&k).is_some());
        state.rollback();
        assert!(state.read_from_cache(&k).is_none());
    }

    #[test]
    fn execution_state_into_sets() {
        let mut data = BTreeMap::new();
        let k1 = cell(1, 0, 0);
        let k2 = cell(1, 1, 0);
        data.insert(k1, 100);
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        let _ = state.read_from_snapshot(&k1, TY, type_runtimes()).unwrap();
        state.write_buffered(&k2, TY, Some(u64_typed(42)));

        let (read_set, write_set) = state.into_sets(type_runtimes()).unwrap();
        assert_eq!(read_set.len(), 1);
        assert_eq!(read_set[0].key, k1);
        assert_eq!(read_set[0].type_id, TY);
        assert_eq!(read_set[0].value, Some(u64_typed(100)));
        assert_eq!(write_set.len(), 1);
        assert_eq!(write_set[0].key, k2);
        assert_eq!(write_set[0].type_id, TY);
        assert_eq!(write_set[0].value, Some(u64_typed(42)));
    }

    #[test]
    fn execution_state_snapshot_type_mismatch_fails_closed() {
        let k = cell(1, 0, 0);
        let snap = CountingSnapshot {
            data: BTreeMap::from([(k, bool_portable(true))]),
            call_count: AtomicU32::new(0),
        };
        let mut state = ExecutionState::new(&snap);

        let err = state
            .read_from_snapshot(&k, TY, type_runtimes())
            .expect_err("type mismatch must fail");
        assert!(matches!(err, TabulaError::TypeMismatch { .. }));
        assert!(state.read_from_cache(&k).is_none());
    }
}
