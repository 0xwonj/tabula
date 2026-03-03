//! Overlay public API integration tests.
//!
//! Tests for private types (ExecutionState, TraceRecorder) remain
//! inline in src/overlay.rs.

mod common;

use std::collections::BTreeMap;

use tabula_core::{Value, ValueType};

use tabula_executor::overlay::Overlay;

use common::*;

const TY: ValueType = ValueType::U64;

// ── Core semantics ──────────────────────────────────────────────────────

#[test]
fn read_your_writes() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(42)), TY);
    let v = ov.read(&k, TY).unwrap();
    assert_eq!(v, Some(Value::U64(42)));
    assert_eq!(snap.call_count(), 0);
}

#[test]
fn read_dedup() {
    let mut data = BTreeMap::new();
    let k = cell(1, 0, 0);
    data.insert(k, Value::U64(100));
    let snap = CountingSnapshot::new(data);
    let mut ov = Overlay::new(&snap);

    let v1 = ov.read(&k, TY).unwrap();
    let v2 = ov.read(&k, TY).unwrap();
    assert_eq!(v1, Some(Value::U64(100)));
    assert_eq!(v2, Some(Value::U64(100)));
    assert_eq!(snap.call_count(), 1);
}

#[test]
fn write_coalescing() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(1)), TY);
    ov.write(&k, Some(Value::U64(2)), TY);

    let result = ov.into_result();
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k, Some(Value::U64(2))));
}

#[test]
fn empty_overlay() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let ov = Overlay::new(&snap);
    let result = ov.into_result();
    assert!(result.read_set_old.is_empty());
    assert!(result.write_set_final.is_empty());
    assert!(result.events.is_empty());
}

#[test]
fn read_absent_cell() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    let v = ov.read(&k, TY).unwrap();
    assert_eq!(v, None);

    let result = ov.into_result();
    assert_eq!(result.events.len(), 1);
    assert!(result.events[0].val_is_null);
    assert_eq!(result.events[0].value, Value::U64(0));
}

#[test]
fn read_set_old_excludes_written_before_read() {
    let mut data = BTreeMap::new();
    let k1 = cell(1, 0, 0);
    let k2 = cell(1, 1, 0);
    data.insert(k1, Value::U64(100));
    data.insert(k2, Value::U64(200));
    let snap = CountingSnapshot::new(data);
    let mut ov = Overlay::new(&snap);

    ov.write(&k1, Some(Value::U64(999)), TY);
    let _ = ov.read(&k1, TY).unwrap();
    let _ = ov.read(&k2, TY).unwrap();

    let result = ov.into_result();
    assert_eq!(result.read_set_old.len(), 1);
    assert_eq!(result.read_set_old[0], (k2, Some(Value::U64(200))));
}

#[test]
fn write_null_then_restore() {
    let mut data = BTreeMap::new();
    let k = cell(1, 0, 0);
    data.insert(k, Value::U64(100));
    let snap = CountingSnapshot::new(data);
    let mut ov = Overlay::new(&snap);

    ov.write(&k, None, TY);
    assert_eq!(ov.read(&k, TY).unwrap(), None);

    ov.write(&k, Some(Value::U64(200)), TY);
    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(200)));
}

// ── Checkpoint / rollback ───────────────────────────────────────────────

#[test]
fn checkpoint_rollback() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(10)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(20)), TY);

    ov.rollback();
    let v = ov.read(&k, TY).unwrap();
    assert_eq!(v, Some(Value::U64(10)));
}

#[test]
fn undo_write_restore() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(10)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(20)), TY);
    ov.write(&k, Some(Value::U64(30)), TY);
    ov.rollback();

    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(10)));
}

#[test]
fn undo_new_key_removal() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.checkpoint();
    ov.write(&k, Some(Value::U64(42)), TY);
    ov.rollback();

    assert_eq!(ov.read(&k, TY).unwrap(), None);
    assert!(ov.into_result().write_set_final.is_empty());
}

#[test]
fn undo_read_cache_removal() {
    let mut data = BTreeMap::new();
    let k = cell(1, 0, 0);
    data.insert(k, Value::U64(100));
    let snap = CountingSnapshot::new(data);
    let mut ov = Overlay::new(&snap);

    ov.checkpoint();
    let _ = ov.read(&k, TY).unwrap();
    assert_eq!(snap.call_count(), 1);
    ov.rollback();

    let _ = ov.read(&k, TY).unwrap();
    assert_eq!(snap.call_count(), 2);
}

#[test]
fn undo_events_truncated() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(1)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(2)), TY);
    ov.write(&k, Some(Value::U64(3)), TY);
    ov.rollback();

    assert_eq!(ov.into_result().events.len(), 1);
}

#[test]
fn discard_clears_undo_log() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.checkpoint();
    ov.write(&k, Some(Value::U64(42)), TY);
    ov.discard_checkpoint();

    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(42)));
}

// ── Nested checkpoints ──────────────────────────────────────────────────

#[test]
fn nested_checkpoint_full_rollback() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(1)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(2)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(3)), TY);

    ov.rollback();
    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(2)));

    ov.rollback();
    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(1)));
}

#[test]
fn nested_checkpoint_inner_discard() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k = cell(1, 0, 0);

    ov.write(&k, Some(Value::U64(1)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(2)), TY);
    ov.checkpoint();
    ov.write(&k, Some(Value::U64(3)), TY);

    ov.discard_checkpoint();
    ov.rollback();

    assert_eq!(ov.read(&k, TY).unwrap(), Some(Value::U64(1)));
}

#[test]
fn nested_checkpoint_new_keys() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let k1 = cell(1, 0, 0);
    let k2 = cell(1, 1, 0);

    ov.checkpoint();
    ov.write(&k1, Some(Value::U64(10)), TY);
    ov.checkpoint();
    ov.write(&k2, Some(Value::U64(20)), TY);

    ov.rollback();
    assert_eq!(ov.read(&k1, TY).unwrap(), Some(Value::U64(10)));
    assert_eq!(ov.read(&k2, TY).unwrap(), None);

    ov.rollback();
    assert!(ov.into_result().write_set_final.is_empty());
}

#[test]
fn rollback_without_checkpoint_returns_none() {
    let snap = CountingSnapshot::new(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    assert_eq!(ov.rollback(), None);
}
