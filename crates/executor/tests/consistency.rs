//! Consistency checker integration tests.

mod common;

use tabula_core::{AccessEvent, CellKey, OpKind, PortableValue, TxResult};

use tabula_executor::consistency::check_consistency;

use common::{bool_portable, bytes32_portable, cell, i64_portable, opt, portable, u64_portable};

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_event(key: CellKey, value: PortableValue, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Read,
        value: portable(value),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

fn write_event(key: CellKey, value: PortableValue, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Write,
        value: portable(value),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

fn null_write_event(key: CellKey, zero: PortableValue, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Write,
        value: portable(zero),
        val_is_null: true,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

fn null_read_event(key: CellKey, zero: PortableValue, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Read,
        value: portable(zero),
        val_is_null: true,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

/// Wrap a flat event slice into a single-tx TxResult for etrace identity checks.
fn single_tx(events: &[AccessEvent]) -> Vec<TxResult> {
    vec![TxResult::success(events.to_vec(), vec![])]
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn valid_trace() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, u64_portable(100), 0),
        write_event(k, u64_portable(80), 1),
        read_event(k, u64_portable(80), 2),
    ];
    let read_set_old = vec![(k, opt(u64_portable(100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn stale_read_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, u64_portable(50), 0),
        read_event(k, u64_portable(100), 1),
    ];
    let read_set_old = vec![(k, opt(u64_portable(100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_err());
}

#[test]
fn write_only_key() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, u64_portable(42), 0),
        write_event(k, u64_portable(99), 1),
    ];
    assert!(check_consistency(&events, &[], &single_tx(&events)).is_ok());
}

#[test]
fn multiple_interleaved_keys() {
    let k1 = cell(1, 0, 0);
    let k2 = cell(1, 1, 0);
    let events = vec![
        read_event(k1, u64_portable(10), 0),
        read_event(k2, u64_portable(20), 1),
        write_event(k1, u64_portable(5), 2),
        read_event(k1, u64_portable(5), 3),
        write_event(k2, u64_portable(25), 4),
        read_event(k2, u64_portable(25), 5),
    ];
    let read_set_old = vec![(k1, opt(u64_portable(10))), (k2, opt(u64_portable(20)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn empty_events() {
    assert!(check_consistency(&[], &[], &[]).is_ok());
}

#[test]
fn invalid_etrace_identity_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        AccessEvent {
            key: k,
            op: OpKind::Read,
            value: portable(u64_portable(10)),
            val_is_null: false,
            time: 0,
            effect_ordinal_in_tx: 0,
        },
        AccessEvent {
            key: k,
            op: OpKind::Write,
            value: portable(u64_portable(11)),
            val_is_null: false,
            time: 1,
            effect_ordinal_in_tx: 2, // skipped 1
        },
    ];
    let read_set_old = vec![(k, opt(u64_portable(10)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_err());
}

#[test]
fn null_write_then_null_read() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, u64_portable(100), 0),
        null_write_event(k, u64_portable(0), 1),
        null_read_event(k, u64_portable(0), 2),
    ];
    let read_set_old = vec![(k, opt(u64_portable(100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn null_write_then_present_read_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        null_write_event(k, u64_portable(0), 0),
        read_event(k, u64_portable(42), 1),
    ];
    let read_set_old = vec![(k, opt(u64_portable(100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_err());
}

#[test]
fn initially_absent_then_write_then_read() {
    let k = cell(1, 0, 0);
    let events = vec![
        null_read_event(k, u64_portable(0), 0),
        write_event(k, u64_portable(42), 1),
        read_event(k, u64_portable(42), 2),
    ];
    let read_set_old = vec![(k, None)];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

// ── Type diversity ──────────────────────────────────────────────────────

#[test]
fn bool_value_consistency() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, bool_portable(true), 0),
        write_event(k, bool_portable(false), 1),
        read_event(k, bool_portable(false), 2),
    ];
    let read_set_old = vec![(k, opt(bool_portable(true)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn i64_value_consistency() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, i64_portable(-100), 0),
        write_event(k, i64_portable(50), 1),
        read_event(k, i64_portable(50), 2),
    ];
    let read_set_old = vec![(k, opt(i64_portable(-100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn bytes32_value_consistency() {
    let k = cell(1, 0, 0);
    let v1 = bytes32_portable([0xaa; 32]);
    let v2 = bytes32_portable([0xbb; 32]);
    let events = vec![
        read_event(k, v1.clone(), 0),
        write_event(k, v2.clone(), 1),
        read_event(k, v2, 2),
    ];
    let read_set_old = vec![(k, opt(v1))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn null_write_value_null_read_value_sequence() {
    let k = cell(1, 0, 0);
    let events = vec![
        read_event(k, u64_portable(100), 0),
        null_write_event(k, u64_portable(0), 1),
        null_read_event(k, u64_portable(0), 2),
        write_event(k, u64_portable(200), 3),
        read_event(k, u64_portable(200), 4),
    ];
    let read_set_old = vec![(k, opt(u64_portable(100)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_ok());
}

#[test]
fn stale_read_i64_fails() {
    let k = cell(1, 0, 0);
    let events = vec![
        write_event(k, i64_portable(50), 0),
        read_event(k, i64_portable(-50), 1),
    ];
    let read_set_old = vec![(k, opt(i64_portable(0)))];
    assert!(check_consistency(&events, &read_set_old, &single_tx(&events)).is_err());
}
