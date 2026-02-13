//! Key-local RAM consistency checker.
//!
//! Validates that execution events satisfy last-write semantics:
//! for each cell key, every read returns the value of the most recent prior write
//! (or the initial value from `read_set_old` if no prior write exists).

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::event::{ExecutionEvent, OpKind};
use tabula_core::types::{CellKey, Value};

/// Check that the execution trace is consistent with last-write semantics.
///
/// - `events`: the full execution event trace (reads and writes, in logical time order)
/// - `read_set_old`: initial values read from committed state (the snapshot)
///
/// Returns `Ok(())` if consistent, or `Err(TabulaError::ConsistencyError)` if a
/// read returns a value inconsistent with the most recent write.
pub fn check_consistency(
    events: &[ExecutionEvent],
    read_set_old: &[(CellKey, Value)],
) -> Result<(), TabulaError> {
    // Build initial value map from read_set_old
    let initial: BTreeMap<CellKey, Value> = read_set_old.iter().cloned().collect();

    // Group events by cell key, preserving time order
    let mut by_key: BTreeMap<CellKey, Vec<&ExecutionEvent>> = BTreeMap::new();
    for event in events {
        by_key.entry(event.key).or_default().push(event);
    }

    // For each key, walk events in time order and verify consistency
    for (key, key_events) in &by_key {
        // Events should already be in time order since they come from execution,
        // but sort to be safe.
        let mut sorted = key_events.clone();
        sorted.sort_by_key(|e| e.time);

        // Current value for this key: starts at the initial/snapshot value
        let mut current_value = initial.get(key).cloned().unwrap_or(Value::Null);

        for event in sorted {
            match event.op {
                OpKind::Write => {
                    current_value = event.value.clone();
                }
                OpKind::Read => {
                    if event.value != current_value {
                        return Err(TabulaError::ConsistencyError(format!(
                            "stale read at key {:?} time {}: expected {:?}, got {:?}",
                            event.key, event.time, current_value, event.value,
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_fixtures::cell;

    fn read_event(key: CellKey, value: Value, time: u64) -> ExecutionEvent {
        ExecutionEvent {
            key,
            op: OpKind::Read,
            value,
            time,
            tx_index: 0,
        }
    }

    fn write_event(key: CellKey, value: Value, time: u64) -> ExecutionEvent {
        ExecutionEvent {
            key,
            op: OpKind::Write,
            value,
            time,
            tx_index: 0,
        }
    }

    #[test]
    fn test_valid_trace() {
        let k = cell(1, 0, 0);
        let events = vec![
            read_event(k, Value::U64(100), 0),
            write_event(k, Value::U64(80), 1),
            read_event(k, Value::U64(80), 2),
        ];
        let read_set_old = vec![(k, Value::U64(100))];
        assert!(check_consistency(&events, &read_set_old).is_ok());
    }

    #[test]
    fn test_stale_read_fails() {
        let k = cell(1, 0, 0);
        let events = vec![
            write_event(k, Value::U64(50), 0),
            read_event(k, Value::U64(100), 1), // stale: should be 50
        ];
        let read_set_old = vec![(k, Value::U64(100))];
        assert!(check_consistency(&events, &read_set_old).is_err());
    }

    #[test]
    fn test_write_only_key() {
        let k = cell(1, 0, 0);
        let events = vec![
            write_event(k, Value::U64(42), 0),
            write_event(k, Value::U64(99), 1),
        ];
        assert!(check_consistency(&events, &[]).is_ok());
    }

    #[test]
    fn test_multiple_interleaved_keys() {
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
        let read_set_old = vec![(k1, Value::U64(10)), (k2, Value::U64(20))];
        assert!(check_consistency(&events, &read_set_old).is_ok());
    }

    #[test]
    fn test_empty_events() {
        assert!(check_consistency(&[], &[]).is_ok());
    }
}
