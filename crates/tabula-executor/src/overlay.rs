//! Local overlay Δ (write-buffer) for deterministic execution.
//!
//! Implements three core semantics rules:
//! - **Read-your-writes**: reads check the write buffer first
//! - **Read deduplication**: reads from snapshot are cached
//! - **Write coalescing**: only the last write per key survives
//!
//! Internally composed of two sub-components:
//! - [`ExecutionState`]: state management (write buffer, read cache, undo log)
//! - [`TraceRecorder`]: event recording (execution trace, logical time)
//!
//! This separation prepares for Phase 4 (ok-gating), where failed-tx
//! rollback will roll back state only while preserving the event trace.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::StateSnapshot;
use tabula_core::{CellKey, ExecutionEvent, LogicalTime, OpKind, Value, ValueType, zero_value};

// ── Undo log ────────────────────────────────────────────────────────────

/// An undo-log entry for reverting a single mutation.
enum UndoEntry {
    /// A write_buffer mutation: key had `prev` value (None = key was absent in buffer).
    Write {
        key: CellKey,
        prev: Option<Option<Value>>,
    },
    /// A read_cache fill: key was absent before this tx.
    ReadCacheFill { key: CellKey },
}

// ── ExecutionState ──────────────────────────────────────────────────────

/// Checkpoint for execution state (undo log position only).
struct StateCheckpoint {
    undo_len: usize,
}

/// State management sub-component of `Overlay`.
///
/// Handles the write buffer, read cache, and undo log for
/// checkpoint/rollback. Does NOT record events.
struct ExecutionState<'a, S: StateSnapshot> {
    snapshot: &'a S,
    write_buffer: BTreeMap<CellKey, Option<Value>>,
    read_cache: BTreeMap<CellKey, Option<Value>>,
    undo_log: Vec<UndoEntry>,
    checkpoints: Vec<StateCheckpoint>,
}

type CellEntries = Vec<(CellKey, Option<Value>)>;

impl<'a, S: StateSnapshot> ExecutionState<'a, S> {
    fn new(snapshot: &'a S) -> Self {
        Self {
            snapshot,
            write_buffer: BTreeMap::new(),
            read_cache: BTreeMap::new(),
            undo_log: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Check the write buffer for a key. Returns `None` if not in buffer.
    fn read_from_buffer(&self, key: &CellKey) -> Option<&Option<Value>> {
        self.write_buffer.get(key)
    }

    /// Check the read cache for a key. Returns `None` if not cached.
    fn read_from_cache(&self, key: &CellKey) -> Option<&Option<Value>> {
        self.read_cache.get(key)
    }

    /// Read from the snapshot, filling the read cache and undo log.
    fn read_from_snapshot(&mut self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        let opt = self.snapshot.read(key)?;
        self.read_cache.insert(*key, opt.clone());
        if !self.checkpoints.is_empty() {
            self.undo_log.push(UndoEntry::ReadCacheFill { key: *key });
        }
        Ok(opt)
    }

    /// Buffer a write, recording the previous value in the undo log.
    fn write_buffered(&mut self, key: &CellKey, value: Option<Value>) {
        if !self.checkpoints.is_empty() {
            let prev = self.write_buffer.get(key).cloned();
            self.undo_log.push(UndoEntry::Write { key: *key, prev });
        }
        self.write_buffer.insert(*key, value);
    }

    fn checkpoint(&mut self) {
        self.checkpoints.push(StateCheckpoint {
            undo_len: self.undo_log.len(),
        });
    }

    fn rollback(&mut self) -> Option<()> {
        let cp = self.checkpoints.pop()?;
        while self.undo_log.len() > cp.undo_len {
            match self.undo_log.pop().unwrap() {
                UndoEntry::Write { key, prev } => match prev {
                    Some(opt_v) => {
                        self.write_buffer.insert(key, opt_v);
                    }
                    None => {
                        self.write_buffer.remove(&key);
                    }
                },
                UndoEntry::ReadCacheFill { key } => {
                    self.read_cache.remove(&key);
                }
            }
        }
        Some(())
    }

    fn discard_checkpoint(&mut self) {
        self.checkpoints.pop();
        if self.checkpoints.is_empty() {
            self.undo_log.clear();
        }
    }

    /// Consume into (read_set_old, write_set_final).
    fn into_sets(self) -> (CellEntries, CellEntries) {
        let read_set_old: Vec<_> = self.read_cache.into_iter().collect();
        let write_set_final: Vec<_> = self.write_buffer.into_iter().collect();
        (read_set_old, write_set_final)
    }
}

// ── TraceRecorder ───────────────────────────────────────────────────────

/// Checkpoint for the trace recorder.
struct RecorderCheckpoint {
    events_len: usize,
    time: LogicalTime,
    tx_index: u32,
}

/// Event recording sub-component of `Overlay`.
///
/// Handles the execution event trace, logical time, and tx index.
/// Accessible as `pub(crate)` for future ok-gating support, where
/// events must be preserved even when state is rolled back.
pub(crate) struct TraceRecorder {
    events: Vec<ExecutionEvent>,
    time: LogicalTime,
    current_tx_index: u32,
    checkpoints: Vec<RecorderCheckpoint>,
}

impl TraceRecorder {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            time: 0,
            current_tx_index: 0,
            checkpoints: Vec::new(),
        }
    }

    /// Record an execution event and advance the logical clock.
    fn record_event(
        &mut self,
        key: &CellKey,
        op: OpKind,
        opt_value: &Option<Value>,
        col_type: ValueType,
    ) {
        let (value, val_is_null) = match opt_value {
            Some(v) => (v.clone(), false),
            None => (zero_value(col_type), true),
        };
        self.events.push(ExecutionEvent {
            key: *key,
            op,
            value,
            val_is_null,
            time: self.time,
            tx_index: self.current_tx_index,
        });
        self.time += 1;
    }

    fn checkpoint(&mut self) {
        self.checkpoints.push(RecorderCheckpoint {
            events_len: self.events.len(),
            time: self.time,
            tx_index: self.current_tx_index,
        });
    }

    fn rollback(&mut self) -> Option<()> {
        let cp = self.checkpoints.pop()?;
        self.events.truncate(cp.events_len);
        self.time = cp.time;
        self.current_tx_index = cp.tx_index;
        Some(())
    }

    fn discard_checkpoint(&mut self) {
        self.checkpoints.pop();
    }

    fn time(&self) -> LogicalTime {
        self.time
    }

    fn set_tx_index(&mut self, idx: u32) {
        self.current_tx_index = idx;
    }

    fn events_len(&self) -> usize {
        self.events.len()
    }

    fn events_since(&self, since: usize) -> Vec<ExecutionEvent> {
        self.events[since..].to_vec()
    }

    fn into_events(self) -> Vec<ExecutionEvent> {
        self.events
    }
}

// ── OverlayResult ───────────────────────────────────────────────────────

/// Finalized overlay output, consumed by the batch executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayResult {
    /// Cells read from committed state (deduplicated). `None` = absent.
    pub read_set_old: Vec<(CellKey, Option<Value>)>,
    /// Final writes to committed state (coalesced). `None` = delete.
    pub write_set_final: Vec<(CellKey, Option<Value>)>,
    /// Full execution trace.
    pub events: Vec<ExecutionEvent>,
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
/// Internally composed of [`ExecutionState`] (state management) and
/// [`TraceRecorder`] (event recording). The public API is unchanged.
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
            let opt = opt.clone();
            self.recorder
                .record_event(key, OpKind::Read, &opt, col_type);
            return Ok(opt);
        }

        // Rule B: read deduplication
        if let Some(opt) = self.state.read_from_cache(key) {
            let opt = opt.clone();
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
    pub fn events_since(&self, since: usize) -> Vec<ExecutionEvent> {
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
    use super::*;

    use crate::test_fixtures::*;

    const TY: ValueType = ValueType::U64;

    // ── Existing Overlay tests (unchanged public API) ───────────────────

    #[test]
    fn test_read_your_writes() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Some(Value::U64(42)), TY);
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, Some(Value::U64(42)));
        // Should NOT have called snapshot
        assert_eq!(snap.call_count(), 0);
    }

    #[test]
    fn test_read_dedup() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut ov = Overlay::new(&snap);

        let v1 = ov.read(&k, TY).unwrap();
        let v2 = ov.read(&k, TY).unwrap();
        assert_eq!(v1, Some(Value::U64(100)));
        assert_eq!(v2, Some(Value::U64(100)));
        // Snapshot should only have been called once
        assert_eq!(snap.call_count(), 1);
    }

    #[test]
    fn test_write_coalescing() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Some(Value::U64(1)), TY);
        ov.write(&k, Some(Value::U64(2)), TY);

        let result = ov.into_result();
        // Only one entry in write_set_final, with the last value
        assert_eq!(result.write_set_final.len(), 1);
        assert_eq!(result.write_set_final[0], (k, Some(Value::U64(2))));
    }

    #[test]
    fn test_checkpoint_rollback() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Some(Value::U64(10)), TY);
        ov.checkpoint();
        ov.write(&k, Some(Value::U64(20)), TY);

        // After rollback, should see the first write
        ov.rollback();
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, Some(Value::U64(10)));
    }

    #[test]
    fn test_read_set_old_excludes_written_before_read() {
        let mut data = BTreeMap::new();
        let k1 = cell(1, 0, 0);
        let k2 = cell(1, 1, 0);
        data.insert(k1, Value::U64(100));
        data.insert(k2, Value::U64(200));
        let snap = CountingSnapshot::new(data);
        let mut ov = Overlay::new(&snap);

        // Write k1 before reading it — should NOT appear in read_set_old
        ov.write(&k1, Some(Value::U64(999)), TY);
        let _ = ov.read(&k1, TY).unwrap();
        // Read k2 from snapshot — should appear in read_set_old
        let _ = ov.read(&k2, TY).unwrap();

        let result = ov.into_result();
        // read_set_old should only contain k2
        assert_eq!(result.read_set_old.len(), 1);
        assert_eq!(result.read_set_old[0], (k2, Some(Value::U64(200))));
    }

    #[test]
    fn test_empty_overlay() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let ov = Overlay::new(&snap);
        let result = ov.into_result();
        assert!(result.read_set_old.is_empty());
        assert!(result.write_set_final.is_empty());
        assert!(result.events.is_empty());
    }

    #[test]
    fn test_undo_write_restore() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Some(Value::U64(10)), TY);
        ov.checkpoint();
        ov.write(&k, Some(Value::U64(20)), TY);
        ov.write(&k, Some(Value::U64(30)), TY);
        ov.rollback();

        // Should see original value
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, Some(Value::U64(10)));
    }

    #[test]
    fn test_undo_new_key_removal() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.checkpoint();
        ov.write(&k, Some(Value::U64(42)), TY);
        ov.rollback();

        // Key should be absent (read from snapshot = None)
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, None);
        let result = ov.into_result();
        assert!(result.write_set_final.is_empty());
    }

    #[test]
    fn test_undo_read_cache_removal() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut ov = Overlay::new(&snap);

        ov.checkpoint();
        let _ = ov.read(&k, TY).unwrap(); // fills read cache
        assert_eq!(snap.call_count(), 1);
        ov.rollback();

        // After rollback, read cache should be cleared for this key,
        // so re-reading should call snapshot again
        let _ = ov.read(&k, TY).unwrap();
        assert_eq!(snap.call_count(), 2);
    }

    #[test]
    fn test_undo_events_truncated() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Some(Value::U64(1)), TY); // event 0
        ov.checkpoint();
        ov.write(&k, Some(Value::U64(2)), TY); // event 1
        ov.write(&k, Some(Value::U64(3)), TY); // event 2
        ov.rollback();

        // Only 1 event should remain
        let result = ov.into_result();
        assert_eq!(result.events.len(), 1);
    }

    #[test]
    fn test_discard_clears_undo_log() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.checkpoint();
        ov.write(&k, Some(Value::U64(42)), TY);
        ov.discard_checkpoint();

        // After discard with no remaining checkpoints, value persists
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, Some(Value::U64(42)));
    }

    #[test]
    fn test_write_null_then_restore() {
        let mut data = BTreeMap::new();
        let k = cell(1, 0, 0);
        data.insert(k, Value::U64(100));
        let snap = CountingSnapshot::new(data);
        let mut ov = Overlay::new(&snap);

        // Write null (delete)
        ov.write(&k, None, TY);
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, None);

        // Write value back
        ov.write(&k, Some(Value::U64(200)), TY);
        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, Some(Value::U64(200)));
    }

    #[test]
    fn test_read_absent_cell() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        let v = ov.read(&k, TY).unwrap();
        assert_eq!(v, None);

        // Event should record val_is_null=true with canonical zero
        let result = ov.into_result();
        assert_eq!(result.events.len(), 1);
        assert!(result.events[0].val_is_null);
        assert_eq!(result.events[0].value, Value::U64(0));
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
        // Now cached
        assert_eq!(state.read_from_cache(&k), Some(&Some(Value::U64(100))));
        // Snapshot called once
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
