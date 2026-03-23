//! Local overlay Δ (write-buffer) for deterministic execution.
//!
//! Implements three core semantics rules:
//! - **Read-your-writes**: reads check the write buffer first
//! - **Read deduplication**: reads from snapshot are cached
//! - **Write coalescing**: only the last write per key survives
//!
//! Internally composed of two sub-components:
//! - [`ExecutionState`](crate::execution_state) — state management (write buffer, read cache, undo log)
//! - [`TraceRecorder`](crate::trace_recorder) — event recording (execution trace, logical time)
//!
//! This separation prepares for Phase 4 (ok-gating), where failed-tx
//! rollback will roll back state only while preserving the event trace.

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{AccessEvent, CellKey, LogicalTime, OpKind, PortableValue, TypeId};
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::execution_state::ExecutionState;
use crate::trace_recorder::TraceRecorder;

// ── OverlayResult ───────────────────────────────────────────────────────

/// Finalized overlay output, consumed by the batch executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayResult {
    /// Cells read from committed state (deduplicated). `None` = absent.
    pub read_set_old: Vec<(CellKey, Option<PortableValue>)>,
    /// Final writes to committed state (coalesced). `None` = delete.
    pub write_set_final: Vec<(CellKey, Option<PortableValue>)>,
    /// Full execution trace.
    pub events: Vec<AccessEvent>,
}

// ── Overlay (public facade) ─────────────────────────────────────────────

/// A local overlay sitting on top of a `StateView`.
///
/// All reads go through the overlay; writes are buffered locally.
/// Supports checkpoint/rollback for per-tx failure recovery.
///
/// Uses an undo-log for O(1) checkpoint and O(k) rollback (where k
/// is the number of mutations since the checkpoint).
///
/// Internally composed of **ExecutionState** (state management) and
/// **TraceRecorder** (event recording). The public API is unchanged.
pub struct Overlay<'a, S: StateView> {
    state: ExecutionState<'a, S>,
    recorder: TraceRecorder,
    type_runtimes: &'a TypeRuntimeRegistry,
}

impl<'a, S: StateView> Overlay<'a, S> {
    /// Create a new overlay on top of a snapshot.
    pub fn new(snapshot: &'a S, type_runtimes: &'a TypeRuntimeRegistry) -> Self {
        Self {
            state: ExecutionState::new(snapshot),
            recorder: TraceRecorder::new(),
            type_runtimes,
        }
    }

    /// Set the current transaction index (called by the batch executor).
    pub fn set_tx_index(&mut self, idx: u32) {
        self.recorder.set_tx_index(idx);
    }

    /// Read a cell: checks write buffer, then read cache, then snapshot.
    ///
    /// `col_type` is needed to produce the canonical zero value for events
    /// when the cell is absent.
    pub fn read(
        &mut self,
        key: &CellKey,
        col_type: TypeId,
    ) -> Result<Option<TypedValue>, TabulaError> {
        // Rule A: read-your-writes
        if let Some(opt) = self.state.read_from_buffer(key) {
            let opt = opt.clone();
            self.recorder
                .record_event(key, OpKind::Read, &opt, col_type, self.type_runtimes)?;
            return Ok(opt);
        }

        // Rule B: read deduplication
        if let Some(opt) = self.state.read_from_cache(key) {
            let opt = opt.clone();
            self.recorder
                .record_event(key, OpKind::Read, &opt, col_type, self.type_runtimes)?;
            return Ok(opt);
        }

        // Cache miss: read from snapshot
        let opt = self.state.read_from_snapshot(key, self.type_runtimes)?;
        self.recorder
            .record_event(key, OpKind::Read, &opt, col_type, self.type_runtimes)?;
        Ok(opt)
    }

    /// Write a value to a cell (buffered locally).
    ///
    /// `value` is `None` for a delete (null write), `Some(v)` for a value write.
    /// `col_type` is needed to produce the canonical zero value for events.
    pub fn write(
        &mut self,
        key: &CellKey,
        value: Option<TypedValue>,
        col_type: TypeId,
    ) -> Result<(), TabulaError> {
        self.recorder
            .record_event(key, OpKind::Write, &value, col_type, self.type_runtimes)?;
        // Rule C: write coalescing — last write wins
        self.state.write_buffered(key, value);
        Ok(())
    }

    /// Save the current overlay state for potential rollback. O(1).
    pub fn checkpoint(&mut self) {
        self.state.checkpoint();
        self.recorder.checkpoint();
    }

    /// Restore the overlay to the most recent checkpoint. O(k).
    ///
    /// Returns `None` if no checkpoint exists.
    pub fn rollback(&mut self) -> Option<()> {
        self.state.rollback()?;
        self.recorder.rollback()?;
        Some(())
    }

    /// Discard the most recent checkpoint (tx succeeded).
    pub fn discard_checkpoint(&mut self) {
        self.state.discard_checkpoint();
        self.recorder.discard_checkpoint();
    }

    /// Current logical time.
    pub fn time(&self) -> LogicalTime {
        self.recorder.time()
    }

    /// Number of events recorded so far.
    pub fn events_len(&self) -> usize {
        self.recorder.events_len()
    }

    /// Clone events recorded since a given index.
    pub fn events_since(&self, since: usize) -> Vec<AccessEvent> {
        self.recorder.events_since(since)
    }

    /// Finalize the overlay into its output components.
    pub fn into_result(self) -> Result<OverlayResult, TabulaError> {
        let (read_set_old, write_set_final) = self.state.into_sets(self.type_runtimes)?;
        Ok(OverlayResult {
            read_set_old,
            write_set_final,
            events: self.recorder.into_events(),
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
    use tabula_core::{CellKey, ColId, OpKind, PortableValue, RowKey, TableId};
    use tabula_profile::TYPE_U64_ID;
    use tabula_types::{TypeRuntimeRegistry, u64_portable, u64_typed};

    use crate::execution_state::ExecutionState;
    use crate::trace_recorder::TraceRecorder;

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

    // ── ExecutionState unit tests ───────────────────────────────────────

    #[test]
    fn execution_state_buffer_roundtrip() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut state = ExecutionState::new(&snap);
        let k = cell(1, 0, 0);

        assert!(state.read_from_buffer(&k).is_none());
        state.write_buffered(&k, Some(u64_typed(42)));
        assert_eq!(state.read_from_buffer(&k), Some(&Some(u64_typed(42))));
    }

    #[test]
    fn execution_state_cache_and_snapshot() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, 100);
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        assert!(state.read_from_cache(&k).is_none());
        let v = state.read_from_snapshot(&k, type_runtimes()).unwrap();
        assert_eq!(v, Some(u64_typed(100)));
        assert_eq!(state.read_from_cache(&k), Some(&Some(u64_typed(100))));
        assert_eq!(snap.call_count(), 1);
    }

    #[test]
    fn execution_state_rollback_restores_buffer() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut state = ExecutionState::new(&snap);
        let k = cell(1, 0, 0);

        state.write_buffered(&k, Some(u64_typed(10)));
        state.checkpoint();
        state.write_buffered(&k, Some(u64_typed(20)));
        state.rollback();

        assert_eq!(state.read_from_buffer(&k), Some(&Some(u64_typed(10))));
    }

    #[test]
    fn execution_state_rollback_clears_new_cache() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, 100);
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        state.checkpoint();
        let _ = state.read_from_snapshot(&k, type_runtimes()).unwrap();
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

        let _ = state.read_from_snapshot(&k1, type_runtimes()).unwrap();
        state.write_buffered(&k2, Some(u64_typed(42)));

        let (read_set, write_set) = state.into_sets(type_runtimes()).unwrap();
        assert_eq!(read_set.len(), 1);
        assert_eq!(read_set[0], (k1, Some(u64_portable(100))));
        assert_eq!(write_set.len(), 1);
        assert_eq!(write_set[0], (k2, Some(u64_portable(42))));
    }

    // ── TraceRecorder unit tests ────────────────────────────────────────

    #[test]
    fn recorder_event_advances_time() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        assert_eq!(rec.time(), 0);
        rec.record_event(&k, OpKind::Read, &Some(u64_typed(1)), TY, type_runtimes())
            .unwrap();
        assert_eq!(rec.time(), 1);
        rec.record_event(&k, OpKind::Write, &Some(u64_typed(2)), TY, type_runtimes())
            .unwrap();
        assert_eq!(rec.time(), 2);
        assert_eq!(rec.events_len(), 2);
    }

    #[test]
    fn recorder_rollback_restores_time_and_events() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        rec.record_event(&k, OpKind::Read, &Some(u64_typed(1)), TY, type_runtimes())
            .unwrap();
        rec.checkpoint();
        rec.record_event(&k, OpKind::Write, &Some(u64_typed(2)), TY, type_runtimes())
            .unwrap();
        rec.record_event(&k, OpKind::Write, &Some(u64_typed(3)), TY, type_runtimes())
            .unwrap();
        assert_eq!(rec.time(), 3);
        assert_eq!(rec.events_len(), 3);

        rec.rollback();
        assert_eq!(rec.time(), 1);
        assert_eq!(rec.events_len(), 1);
    }

    #[test]
    fn recorder_tx_index_and_events_since() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        rec.set_tx_index(0);
        rec.record_event(&k, OpKind::Read, &Some(u64_typed(1)), TY, type_runtimes())
            .unwrap();
        let since = rec.events_len();
        rec.set_tx_index(1);
        rec.record_event(&k, OpKind::Write, &Some(u64_typed(2)), TY, type_runtimes())
            .unwrap();

        let recent = rec.events_since(since);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].effect_ordinal_in_tx, 0);
    }

    #[test]
    fn recorder_null_event_records_canonical_zero() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        rec.record_event(&k, OpKind::Read, &None, TY, type_runtimes())
            .unwrap();
        let events = rec.into_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].val_is_null);
        assert_eq!(events[0].value, u64_portable(0));
    }
}
