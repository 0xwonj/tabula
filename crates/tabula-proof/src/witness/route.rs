//! Key routing for the witness pipeline.
//!
//! Routes each accessed `CellKey` to the cheapest valid memory-layer proof path.
//! Read-only keys use cheaper opening proofs (no state update required).

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{CellKey, ExecutionResult};

/// Access pattern for keys on a short-run proof path.
///
/// Determines which ShortRunChip variant handles the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessPattern {
    /// Init + read + write (most common short-run pattern).
    InitReadWrite,
    /// Init + write only (blind write with no preceding read).
    InitWrite,
}

/// Memory-layer proof path for a cell key within a batch.
///
/// Classification priority: `ReadOnly` > `ShortRun` > `SortedMemory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyRoute {
    /// Key was read but never written in `write_set_final`.
    /// Eligible for read-only opening proofs.
    ReadOnly,
    /// Key has a short, predictable access pattern within a single tx.
    /// Eligible for a dedicated ShortRunChip (cheaper than GlobalSortedMem).
    ShortRun(AccessPattern),
    /// Key appears in `write_set_final` (may also have been read).
    /// Requires full state-update proof via GlobalSortedMem.
    SortedMemory,
}

/// Route every accessed key in an `ExecutionResult` to its proof path.
///
/// A key is `SortedMemory` if it appears in `write_set_final`, regardless of
/// whether it was also read. A key is `ReadOnly` if it was accessed
/// (appears in events) but not in `write_set_final`.
///
/// Keys that appear only in `write_set_final` (blind writes) are also
/// routed as `SortedMemory`.
///
/// # Future: ShortRun routing
///
/// `ShortRun` classification is not yet implemented. All written keys
/// are currently routed to `SortedMemory`. Phase 2 will add heuristics
/// to promote eligible keys to `ShortRun(AccessPattern)`.
///
/// # Invariant assumption
///
/// This function assumes `result.events` contains only events from
/// **successful** transactions. Failed-tx events live in
/// `TxOutcome::Failed.partial_events` and are excluded by the executor's
/// rollback. If this invariant were violated (failed-tx write events in
/// `result.events`), a key with a rolled-back write could be mis-routed
/// as `ReadOnly` despite having a write access row in the execution trace.
pub fn route_keys(result: &ExecutionResult) -> BTreeMap<CellKey, KeyRoute> {
    let written: BTreeSet<CellKey> = result.write_set_final.iter().map(|(key, _)| *key).collect();

    let mut routes = BTreeMap::new();

    // All event keys: ReadOnly unless overridden by write_set_final.
    // `or_insert` is correct: every event for the same key produces the same route
    // because routing is determined solely by write_set_final membership, not event type.
    for event in &result.events {
        routes
            .entry(event.key)
            .or_insert(if written.contains(&event.key) {
                KeyRoute::SortedMemory
            } else {
                KeyRoute::ReadOnly
            });
    }

    // Blind writes: keys in write_set_final but not in events.
    for (key, _) in &result.write_set_final {
        routes.entry(*key).or_insert(KeyRoute::SortedMemory);
    }

    routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::{ColId, ExecutionEvent, OpKind, RowKey, TableId, TxOutcome, Value};

    fn ck(table: u32, col: u16, row: u64) -> CellKey {
        CellKey {
            table: TableId(table),
            col: ColId(col),
            row: RowKey(row),
        }
    }

    fn read_ev(key: CellKey, time: u64) -> ExecutionEvent {
        ExecutionEvent {
            key,
            op: OpKind::Read,
            value: Value::U64(0),
            val_is_null: false,
            time,
            tx_index: 0,
        }
    }

    fn write_ev(key: CellKey, time: u64) -> ExecutionEvent {
        ExecutionEvent {
            key,
            op: OpKind::Write,
            value: Value::U64(0),
            val_is_null: false,
            time,
            tx_index: 0,
        }
    }

    fn empty_result() -> ExecutionResult {
        ExecutionResult {
            read_set_old: vec![],
            write_set_final: vec![],
            events: vec![],
            emitted: vec![],
            tx_outcomes: vec![],
        }
    }

    #[test]
    fn read_only_keys_no_writes() {
        let k1 = ck(1, 0, 1);
        let k2 = ck(1, 0, 2);
        let result = ExecutionResult {
            events: vec![read_ev(k1, 1), read_ev(k2, 2)],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[&k1], KeyRoute::ReadOnly);
        assert_eq!(routes[&k2], KeyRoute::ReadOnly);
    }

    #[test]
    fn sorted_memory_keys_with_writes() {
        let k1 = ck(1, 0, 1);
        let result = ExecutionResult {
            events: vec![read_ev(k1, 1), write_ev(k1, 2)],
            write_set_final: vec![(k1, Some(Value::U64(42)))],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[&k1], KeyRoute::SortedMemory);
    }

    #[test]
    fn mixed_routing() {
        let k_read = ck(1, 0, 1);
        let k_write = ck(1, 0, 2);
        let result = ExecutionResult {
            events: vec![
                read_ev(k_read, 1),
                read_ev(k_write, 2),
                write_ev(k_write, 3),
            ],
            write_set_final: vec![(k_write, Some(Value::U64(99)))],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[&k_read], KeyRoute::ReadOnly);
        assert_eq!(routes[&k_write], KeyRoute::SortedMemory);
    }

    #[test]
    fn blind_write_routed_sorted_memory() {
        let k_blind = ck(1, 0, 5);
        let result = ExecutionResult {
            write_set_final: vec![(k_blind, Some(Value::U64(1)))],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[&k_blind], KeyRoute::SortedMemory);
    }

    #[test]
    fn empty_result_empty_map() {
        let routes = route_keys(&empty_result());
        assert!(routes.is_empty());
    }

    #[test]
    fn deterministic_output() {
        let k1 = ck(1, 0, 1);
        let k2 = ck(1, 1, 2);
        let result = ExecutionResult {
            events: vec![read_ev(k1, 1), read_ev(k2, 2), write_ev(k2, 3)],
            write_set_final: vec![(k2, Some(Value::U64(10)))],
            tx_outcomes: vec![TxOutcome::Success],
            ..empty_result()
        };
        let r1 = route_keys(&result);
        let r2 = route_keys(&result);
        assert_eq!(r1, r2);
    }

    #[test]
    fn read_of_written_key_is_sorted_memory() {
        let k = ck(1, 0, 1);
        let result = ExecutionResult {
            events: vec![read_ev(k, 1), write_ev(k, 2)],
            write_set_final: vec![(k, Some(Value::U64(100)))],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes[&k], KeyRoute::SortedMemory);
    }

    #[test]
    fn multi_tx_read_only_same_key() {
        // Same key read by two different txs, no writes -> still ReadOnly.
        let k = ck(1, 0, 1);
        let mut ev0 = read_ev(k, 1);
        ev0.tx_index = 0;
        let mut ev1 = read_ev(k, 3);
        ev1.tx_index = 1;
        let result = ExecutionResult {
            events: vec![ev0, ev1],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[&k], KeyRoute::ReadOnly);
    }

    #[test]
    fn delete_routed_sorted_memory() {
        // write_set_final with None (delete) -> General.
        let k = ck(1, 0, 1);
        let result = ExecutionResult {
            events: vec![read_ev(k, 1), write_ev(k, 2)],
            write_set_final: vec![(k, None)],
            ..empty_result()
        };
        let routes = route_keys(&result);
        assert_eq!(routes[&k], KeyRoute::SortedMemory);
    }
}
