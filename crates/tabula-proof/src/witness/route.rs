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
