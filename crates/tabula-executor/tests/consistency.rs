//! Consistency checker integration tests.

mod common;

use tabula_core::{CellKey, ExecutionEvent, OpKind, Value};

use tabula_executor::consistency::check_consistency;

use common::cell;

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_event(key: CellKey, value: Value, time: u64) -> ExecutionEvent {
    ExecutionEvent {
        key,
        op: OpKind::Read,
        value,
        val_is_null: false,
        time,
        tx_index: 0,
    }
}

fn write_event(key: CellKey, value: Value, time: u64) -> ExecutionEvent {
    ExecutionEvent {
        key,
        op: OpKind::Write,
        value,
        val_is_null: false,
        time,
        tx_index: 0,
    }
}

fn null_write_event(key: CellKey, zero: Value, time: u64) -> ExecutionEvent {
    ExecutionEvent {
        key,
        op: OpKind::Write,
        value: zero,
        val_is_null: true,
        time,
        tx_index: 0,
    }
}

fn null_read_event(key: CellKey, zero: Value, time: u64) -> ExecutionEvent {
    ExecutionEvent {
        key,
        op: OpKind::Read,
        value: zero,
        val_is_null: true,
        time,
        tx_index: 0,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn valid_trace() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, Value::U64(100), 0),
        write_event(k, Value::U64(80), 1),
        read_event(k, Value::U64(80), 2),
    ];
    let read_set_old = vec![(k, Some(Value::U64(100)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn stale_read_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, Value::U64(50), 0),
        read_event(k, Value::U64(100), 1),
    ];
    let read_set_old = vec![(k, Some(Value::U64(100)))];
    assert!(check_consistency(&events, &read_set_old).is_err());
}

#[test]
fn write_only_key() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, Value::U64(42), 0),
        write_event(k, Value::U64(99), 1),
    ];
    assert!(check_consistency(&events, &[]).is_ok());
}

#[test]
fn multiple_interleaved_keys() {
    let k1 = cell(1, 0, 0);
    let k2 = cell(1, 1, 0);
    let events = vec![
        read_event(k1, Value::U64(10), 0),
        read_event(k2, Value::U64(20), 1),
        write_event(k1, Value::U64(5), 2),
        read_event(k1, Value::U64(5), 3),
        write_event(k2, Value::U64(25), 4),
        read_event(k2, Value::U64(25), 5),
    ];
    let read_set_old = vec![(k1, Some(Value::U64(10))), (k2, Some(Value::U64(20)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn empty_events() {
    assert!(check_consistency(&[], &[]).is_ok());
}

#[test]
fn null_write_then_null_read() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, Value::U64(100), 0),
        null_write_event(k, Value::U64(0), 1),
        null_read_event(k, Value::U64(0), 2),
    ];
    let read_set_old = vec![(k, Some(Value::U64(100)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn null_write_then_present_read_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        null_write_event(k, Value::U64(0), 0),
        read_event(k, Value::U64(42), 1),
    ];
    let read_set_old = vec![(k, Some(Value::U64(100)))];
    assert!(check_consistency(&events, &read_set_old).is_err());
}

#[test]
fn initially_absent_then_write_then_read() {
    let k = cell(1, 0, 0);
    let events = vec![
        null_read_event(k, Value::U64(0), 0),
        write_event(k, Value::U64(42), 1),
        read_event(k, Value::U64(42), 2),
    ];
    let read_set_old = vec![(k, None)];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

// ── Type diversity ──────────────────────────────────────────────────────

#[test]
fn bool_value_consistency() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, Value::Bool(true), 0),
        write_event(k, Value::Bool(false), 1),
        read_event(k, Value::Bool(false), 2),
    ];
    let read_set_old = vec![(k, Some(Value::Bool(true)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn i64_value_consistency() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, Value::I64(-100), 0),
        write_event(k, Value::I64(50), 1),
        read_event(k, Value::I64(50), 2),
    ];
    let read_set_old = vec![(k, Some(Value::I64(-100)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn bytes32_value_consistency() {
    let k = cell(1, 0, 0);
    let v1 = Value::Bytes32([0xaa; 32]);
    let v2 = Value::Bytes32([0xbb; 32]);
    let events = vec![
        read_event(k, v1, 0),
        write_event(k, v2, 1),
        read_event(k, v2, 2),
    ];
    let read_set_old = vec![(k, Some(v1))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn null_write_value_null_read_value_sequence() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, Value::U64(100), 0),
        null_write_event(k, Value::U64(0), 1),
        null_read_event(k, Value::U64(0), 2),
        write_event(k, Value::U64(200), 3),
        read_event(k, Value::U64(200), 4),
    ];
    let read_set_old = vec![(k, Some(Value::U64(100)))];
    assert!(check_consistency(&events, &read_set_old).is_ok());
}

#[test]
fn stale_read_i64_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, Value::I64(50), 0),
        read_event(k, Value::I64(-50), 1),
    ];
    let read_set_old = vec![(k, Some(Value::I64(0)))];
    assert!(check_consistency(&events, &read_set_old).is_err());
}
