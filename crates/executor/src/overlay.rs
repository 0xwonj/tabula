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
use tabula_core::traits::StateSnapshot;
use tabula_core::{CellKey, AccessEvent, LogicalTime, OpKind, Value, ValueType};

use crate::execution_state::ExecutionState;
use crate::trace_recorder::TraceRecorder;

// ── OverlayResult ───────────────────────────────────────────────────────

/// Finalized overlay output, consumed by the batch executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayResult {
    /// Cells read from committed state (deduplicated). `None` = absent.
    pub read_set_old: Vec<(CellKey, Option<Value>)>,
    /// Final writes to committed state (coalesced). `None` = delete.
    pub write_set_final: Vec<(CellKey, Option<Value>)>,
    /// Full execution trace.
    pub events: Vec<AccessEvent>,
}

// ── Overlay (public facade) ─────────────────────────────────────────────

/// A local overlay sitting on top of a `StateSnapshot`.
///
/// All reads go through the overlay; writes are buffered locally.
/// Supports checkpoint/rollback for per-tx failure recovery.
///
/// Uses an undo-log for O(1) checkpoint and O(k) rollback (where k
/// is the number of mutations since the checkpoint).
///
/// Internally composed of **ExecutionState** (state management) and
/// **TraceRecorder** (event recording). The public API is unchanged.
pub struct Overlay<'a, S: StateSnapshot> {
    state: ExecutionState<'a, S>,
    recorder: TraceRecorder,
}

impl<'a, S: StateSnapshot> Overlay<'a, S> {
    /// Create a new overlay on top of a snapshot.
    pub fn new(snapshot: &'a S) -> Self {
        Self {
            state: ExecutionState::new(snapshot),
            recorder: TraceRecorder::new(),
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
        col_type: ValueType,
    ) -> Result<Option<Value>, TabulaError> {
        // Rule A: read-your-writes
        if let Some(opt) = self.state.read_from_buffer(key) {
            let opt = *opt;
            self.recorder
                .record_event(key, OpKind::Read, &opt, col_type);
            return Ok(opt);
        }

        // Rule B: read deduplication
        if let Some(opt) = self.state.read_from_cache(key) {
            let opt = *opt;
            self.recorder
                .record_event(key, OpKind::Read, &opt, col_type);
            return Ok(opt);
        }

        // Cache miss: read from snapshot
        let opt = self.state.read_from_snapshot(key)?;
        self.recorder
            .record_event(key, OpKind::Read, &opt, col_type);
        Ok(opt)
    }

    /// Write a value to a cell (buffered locally).
    ///
    /// `value` is `None` for a delete (null write), `Some(v)` for a value write.
    /// `col_type` is needed to produce the canonical zero value for events.
    pub fn write(&mut self, key: &CellKey, value: Option<Value>, col_type: ValueType) {
        self.recorder
            .record_event(key, OpKind::Write, &value, col_type);
        // Rule C: write coalescing — last write wins
        self.state.write_buffered(key, value);
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
    pub fn into_result(self) -> OverlayResult {
        let (read_set_old, write_set_final) = self.state.into_sets();
        OverlayResult {
            read_set_old,
            write_set_final,
            events: self.recorder.into_events(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    use tabula_core::error::TabulaError;
    use tabula_core::traits::StateSnapshot;
    use tabula_core::{CellKey, ColId, OpKind, RowKey, TableId, Value, ValueType};

    use crate::execution_state::ExecutionState;
    use crate::trace_recorder::TraceRecorder;

    const TY: ValueType = ValueType::U64;

    fn cell(t: u32, r: u64, c: u16) -> CellKey {
        CellKey {
            table: TableId(t),
            col: ColId(c),
            row: RowKey(r),
        }
    }

    struct CountingSnapshot {
        data: BTreeMap<CellKey, Value>,
        call_count: AtomicU32,
    }

    impl CountingSnapshot {
        fn new(data: BTreeMap<CellKey, Value>) -> Self {
            Self {
                data,
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    impl StateSnapshot for CountingSnapshot {
        fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(self.data.get(key).copied())
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
        state.write_buffered(&k, Some(Value::U64(42)));
        assert_eq!(state.read_from_buffer(&k), Some(&Some(Value::U64(42))));
    }

    #[test]
    fn execution_state_cache_and_snapshot() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        assert!(state.read_from_cache(&k).is_none());
        let v = state.read_from_snapshot(&k).unwrap();
        assert_eq!(v, Some(Value::U64(100)));
        assert_eq!(state.read_from_cache(&k), Some(&Some(Value::U64(100))));
        assert_eq!(snap.call_count(), 1);
    }

    #[test]
    fn execution_state_rollback_restores_buffer() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut state = ExecutionState::new(&snap);
        let k = cell(1, 0, 0);

        state.write_buffered(&k, Some(Value::U64(10)));
        state.checkpoint();
        state.write_buffered(&k, Some(Value::U64(20)));
        state.rollback();

        assert_eq!(state.read_from_buffer(&k), Some(&Some(Value::U64(10))));
    }

    #[test]
    fn execution_state_rollback_clears_new_cache() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        state.checkpoint();
        let _ = state.read_from_snapshot(&k).unwrap();
        assert!(state.read_from_cache(&k).is_some());
        state.rollback();
        assert!(state.read_from_cache(&k).is_none());
    }

    #[test]
    fn execution_state_into_sets() {
        let mut data = BTreeMap::new();
        let k1 = cell(1, 0, 0);
        let k2 = cell(1, 1, 0);
        data.insert(k1, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut state = ExecutionState::new(&snap);

        let _ = state.read_from_snapshot(&k1).unwrap();
        state.write_buffered(&k2, Some(Value::U64(42)));

        let (read_set, write_set) = state.into_sets();
        assert_eq!(read_set.len(), 1);
        assert_eq!(read_set[0], (k1, Some(Value::U64(100))));
        assert_eq!(write_set.len(), 1);
        assert_eq!(write_set[0], (k2, Some(Value::U64(42))));
    }

    // ── TraceRecorder unit tests ────────────────────────────────────────

    #[test]
    fn recorder_event_advances_time() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        assert_eq!(rec.time(), 0);
        rec.record_event(&k, OpKind::Read, &Some(Value::U64(1)), TY);
        assert_eq!(rec.time(), 1);
        rec.record_event(&k, OpKind::Write, &Some(Value::U64(2)), TY);
        assert_eq!(rec.time(), 2);
        assert_eq!(rec.events_len(), 2);
    }

    #[test]
    fn recorder_rollback_restores_time_and_events() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        rec.record_event(&k, OpKind::Read, &Some(Value::U64(1)), TY);
        rec.checkpoint();
        rec.record_event(&k, OpKind::Write, &Some(Value::U64(2)), TY);
        rec.record_event(&k, OpKind::Write, &Some(Value::U64(3)), TY);
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
        rec.record_event(&k, OpKind::Read, &Some(Value::U64(1)), TY);
        let since = rec.events_len();
        rec.set_tx_index(1);
        rec.record_event(&k, OpKind::Write, &Some(Value::U64(2)), TY);

        let recent = rec.events_since(since);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tx_index, 1);
    }

    #[test]
    fn recorder_null_event_records_canonical_zero() {
        let mut rec = TraceRecorder::new();
        let k = cell(1, 0, 0);

        rec.record_event(&k, OpKind::Read, &None, TY);
        let events = rec.into_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].val_is_null);
        assert_eq!(events[0].value, Value::U64(0));
    }
}
