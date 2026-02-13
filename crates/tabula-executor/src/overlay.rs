//! Local overlay Δ (write-buffer) for deterministic execution.
//!
//! Implements three core semantics rules:
//! - **Read-your-writes**: reads check the write buffer first
//! - **Read deduplication**: reads from snapshot are cached
//! - **Write coalescing**: only the last write per key survives

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::event::{ExecutionEvent, LogicalTime, OpKind};
use tabula_core::traits::StateSnapshot;
use tabula_core::types::{CellKey, Value};

/// An undo-log entry for reverting a single mutation.
enum UndoEntry {
    /// A write_buffer mutation: key had `prev` value (None = key was absent).
    Write { key: CellKey, prev: Option<Value> },
    /// A read_cache fill: key was absent before this tx.
    ReadCacheFill { key: CellKey },
}

/// Lightweight checkpoint: records positions, not full clones.
struct Checkpoint {
    undo_len: usize,
    events_len: usize,
    time: LogicalTime,
    tx_index: u32,
}

/// Finalized overlay output, consumed by the batch executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayResult {
    /// Cells read from committed state (deduplicated).
    pub read_set_old: Vec<(CellKey, Value)>,
    /// Final writes to committed state (coalesced).
    pub write_set_final: Vec<(CellKey, Value)>,
    /// Full execution trace.
    pub events: Vec<ExecutionEvent>,
}

/// A local overlay sitting on top of a `StateSnapshot`.
///
/// All reads go through the overlay; writes are buffered locally.
/// Supports checkpoint/rollback for per-tx failure recovery.
///
/// Uses an undo-log for O(1) checkpoint and O(k) rollback (where k
/// is the number of mutations since the checkpoint).
pub struct Overlay<'a, S: StateSnapshot> {
    snapshot: &'a S,
    write_buffer: BTreeMap<CellKey, Value>,
    read_cache: BTreeMap<CellKey, Value>,
    events: Vec<ExecutionEvent>,
    time: LogicalTime,
    checkpoints: Vec<Checkpoint>,
    undo_log: Vec<UndoEntry>,
    current_tx_index: u32,
}

impl<'a, S: StateSnapshot> Overlay<'a, S> {
    /// Create a new overlay on top of a snapshot.
    pub fn new(snapshot: &'a S) -> Self {
        Self {
            snapshot,
            write_buffer: BTreeMap::new(),
            read_cache: BTreeMap::new(),
            events: Vec::new(),
            time: 0,
            checkpoints: Vec::new(),
            undo_log: Vec::new(),
            current_tx_index: 0,
        }
    }

    /// Set the current transaction index (called by the batch executor).
    pub fn set_tx_index(&mut self, idx: u32) {
        self.current_tx_index = idx;
    }

    /// Read a cell: checks write buffer, then read cache, then snapshot.
    pub fn read(&mut self, key: &CellKey) -> Result<Value, TabulaError> {
        // Rule A: read-your-writes
        if let Some(v) = self.write_buffer.get(key) {
            let value = v.clone();
            self.record_event(key, OpKind::Read, &value);
            return Ok(value);
        }

        // Rule B: read deduplication
        if let Some(v) = self.read_cache.get(key) {
            let value = v.clone();
            self.record_event(key, OpKind::Read, &value);
            return Ok(value);
        }

        // Cache miss: read from snapshot
        let value = self.snapshot.read(key)?;
        self.read_cache.insert(*key, value.clone());
        if !self.checkpoints.is_empty() {
            self.undo_log.push(UndoEntry::ReadCacheFill { key: *key });
        }
        self.record_event(key, OpKind::Read, &value);
        Ok(value)
    }

    /// Write a value to a cell (buffered locally).
    pub fn write(&mut self, key: &CellKey, value: Value) {
        if !self.checkpoints.is_empty() {
            let prev = self.write_buffer.get(key).cloned();
            self.undo_log.push(UndoEntry::Write { key: *key, prev });
        }
        self.record_event(key, OpKind::Write, &value);
        // Rule C: write coalescing — last write wins
        self.write_buffer.insert(*key, value);
    }

    /// Save the current overlay state for potential rollback. O(1).
    pub fn checkpoint(&mut self) {
        self.checkpoints.push(Checkpoint {
            undo_len: self.undo_log.len(),
            events_len: self.events.len(),
            time: self.time,
            tx_index: self.current_tx_index,
        });
    }

    /// Restore the overlay to the most recent checkpoint. O(k).
    ///
    /// Returns `None` if no checkpoint exists.
    pub fn rollback(&mut self) -> Option<()> {
        let cp = self.checkpoints.pop()?;

        // Replay undo log in reverse
        while self.undo_log.len() > cp.undo_len {
            match self.undo_log.pop().unwrap() {
                UndoEntry::Write { key, prev } => match prev {
                    Some(v) => {
                        self.write_buffer.insert(key, v);
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

        self.events.truncate(cp.events_len);
        self.time = cp.time;
        self.current_tx_index = cp.tx_index;
        Some(())
    }

    /// Discard the most recent checkpoint (tx succeeded).
    pub fn discard_checkpoint(&mut self) {
        self.checkpoints.pop();
        // Clear undo log if no remaining checkpoints
        if self.checkpoints.is_empty() {
            self.undo_log.clear();
        }
    }

    /// Current logical time.
    pub fn time(&self) -> LogicalTime {
        self.time
    }

    /// Number of events recorded so far.
    pub fn events_len(&self) -> usize {
        self.events.len()
    }

    /// Clone events recorded since a given index.
    pub fn events_since(&self, since: usize) -> Vec<ExecutionEvent> {
        self.events[since..].to_vec()
    }

    /// Finalize the overlay into its output components.
    pub fn into_result(self) -> OverlayResult {
        let read_set_old: Vec<(CellKey, Value)> = self.read_cache.into_iter().collect();
        let write_set_final: Vec<(CellKey, Value)> = self.write_buffer.into_iter().collect();
        OverlayResult {
            read_set_old,
            write_set_final,
            events: self.events,
        }
    }

    fn record_event(&mut self, key: &CellKey, op: OpKind, value: &Value) {
        self.events.push(ExecutionEvent {
            key: *key,
            op,
            value: value.clone(),
            time: self.time,
            tx_index: self.current_tx_index,
        });
        self.time += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_fixtures::*;

    #[test]
    fn test_read_your_writes() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Value::U64(42));
        let v = ov.read(&k).unwrap();
        assert_eq!(v, Value::U64(42));
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

        let v1 = ov.read(&k).unwrap();
        let v2 = ov.read(&k).unwrap();
        assert_eq!(v1, Value::U64(100));
        assert_eq!(v2, Value::U64(100));
        // Snapshot should only have been called once
        assert_eq!(snap.call_count(), 1);
    }

    #[test]
    fn test_write_coalescing() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Value::U64(1));
        ov.write(&k, Value::U64(2));

        let result = ov.into_result();
        // Only one entry in write_set_final, with the last value
        assert_eq!(result.write_set_final.len(), 1);
        assert_eq!(result.write_set_final[0], (k, Value::U64(2)));
    }

    #[test]
    fn test_checkpoint_rollback() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Value::U64(10));
        ov.checkpoint();
        ov.write(&k, Value::U64(20));

        // After rollback, should see the first write
        ov.rollback();
        let v = ov.read(&k).unwrap();
        assert_eq!(v, Value::U64(10));
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
        ov.write(&k1, Value::U64(999));
        let _ = ov.read(&k1).unwrap();
        // Read k2 from snapshot — should appear in read_set_old
        let _ = ov.read(&k2).unwrap();

        let result = ov.into_result();
        // read_set_old should only contain k2
        assert_eq!(result.read_set_old.len(), 1);
        assert_eq!(result.read_set_old[0], (k2, Value::U64(200)));
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

        ov.write(&k, Value::U64(10));
        ov.checkpoint();
        ov.write(&k, Value::U64(20));
        ov.write(&k, Value::U64(30));
        ov.rollback();

        // Should see original value
        let v = ov.read(&k).unwrap();
        assert_eq!(v, Value::U64(10));
    }

    #[test]
    fn test_undo_new_key_removal() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.checkpoint();
        ov.write(&k, Value::U64(42));
        ov.rollback();

        // Key should be absent (read from snapshot = Null)
        let v = ov.read(&k).unwrap();
        assert_eq!(v, Value::Null);
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
        let _ = ov.read(&k).unwrap(); // fills read cache
        assert_eq!(snap.call_count(), 1);
        ov.rollback();

        // After rollback, read cache should be cleared for this key,
        // so re-reading should call snapshot again
        let _ = ov.read(&k).unwrap();
        assert_eq!(snap.call_count(), 2);
    }

    #[test]
    fn test_undo_events_truncated() {
        let snap = CountingSnapshot::new(BTreeMap::new());
        let mut ov = Overlay::new(&snap);
        let k = cell(1, 0, 0);

        ov.write(&k, Value::U64(1)); // event 0
        ov.checkpoint();
        ov.write(&k, Value::U64(2)); // event 1
        ov.write(&k, Value::U64(3)); // event 2
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
        ov.write(&k, Value::U64(42));
        ov.discard_checkpoint();

        // After discard with no remaining checkpoints, value persists
        let v = ov.read(&k).unwrap();
        assert_eq!(v, Value::U64(42));
    }
}
