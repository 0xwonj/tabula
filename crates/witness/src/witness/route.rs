//! Key routing for the witness pipeline.
//!
//! Routes each accessed `CellKey` to the cheapest valid memory-layer proof path.
//! Read-only keys use cheaper opening proofs (no state update required).

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{BatchResult, CellKey};

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

/// Route every accessed key in a `BatchResult` to its proof path.
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
/// # Invariant
///
/// Only events from successful transactions are considered. Failed-tx events
/// live in `TxResult::Failed { partial_events, .. }` and are excluded by
/// `successful_events()`. This ensures a key with a rolled-back write
/// is not mis-routed.
pub fn route_keys(result: &BatchResult) -> BTreeMap<CellKey, KeyRoute> {
    let written: BTreeSet<CellKey> = result.write_set_final.iter().map(|(key, _)| *key).collect();

    let mut routes = BTreeMap::new();

    // All event keys: ReadOnly unless overridden by write_set_final.
    // `or_insert` is correct: every event for the same key produces the same route
    // because routing is determined solely by write_set_final membership, not event type.
    for event in result.successful_events() {
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
