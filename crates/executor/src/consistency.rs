//! Key-local RAM consistency checker.
//!
//! Validates that execution events satisfy last-write semantics:
//! for each cell key, every read returns the value of the most recent prior write
//! (or the initial value from `read_set_old` if no prior write exists).

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{
    AccessEvent, CellKey, ExecutionConsistencyStatus, OpKind, PortableValue, TxResult,
};

/// Check that the execution trace is consistent with last-write semantics.
///
/// - `events`: the full execution event trace (reads and writes, in logical time order)
/// - `read_set_old`: initial values read from committed state (the snapshot)
///
/// Returns `Ok(())` if consistent, or `Err(TabulaError::ConsistencyError)` if a
/// read returns a value inconsistent with the most recent write.
pub fn check_consistency(
    events: &[AccessEvent],
    read_set_old: &[(CellKey, Option<PortableValue>)],
    txs: &[TxResult],
) -> Result<(), TabulaError> {
    check_etrace_identity(txs)?;

    // Build initial value map from read_set_old
    let initial: BTreeMap<CellKey, Option<PortableValue>> = read_set_old.iter().cloned().collect();

    // Group events by cell key, preserving time order
    let mut by_key: BTreeMap<CellKey, Vec<&AccessEvent>> = BTreeMap::new();
    for event in events {
        by_key.entry(event.key).or_default().push(event);
    }

    // Convert an event's (value, val_is_null) pair to Option<PortableValue>.
    fn event_to_opt(event: &AccessEvent) -> Option<PortableValue> {
        if event.val_is_null {
            None
        } else {
            Some(event.value.clone())
        }
    }

    // For each key, walk events in time order and verify consistency
    for (key, key_events) in &by_key {
        // Events come from TraceRecorder which monotonically advances time.
        // Assert ordering rather than silently sorting (sorting would mask bugs).
        debug_assert!(
            key_events.windows(2).all(|w| w[0].time <= w[1].time),
            "events for key {key:?} are not in time order"
        );

        // Current value for this key: starts at the initial/snapshot value
        let mut current_opt = initial.get(key).cloned().unwrap_or(None);

        for event in key_events {
            match event.op {
                OpKind::Write => {
                    current_opt = event_to_opt(event);
                }
                OpKind::Read => {
                    let read_opt = event_to_opt(event);
                    if read_opt != current_opt {
                        return Err(TabulaError::ConsistencyError(format!(
                            "stale read at key {:?} time {}: expected {:?}, got {:?}",
                            event.key, event.time, current_opt, read_opt,
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate canonical E-Trace identity constraints.
///
/// For each successful transaction:
/// - first event has `effect_ordinal_in_tx = 0`
/// - ordinals increase contiguously by 1 in event order
pub fn check_etrace_identity(txs: &[TxResult]) -> Result<(), TabulaError> {
    for (tx_idx, tx) in txs.iter().enumerate() {
        if let TxResult::Success { access_trace, .. } = tx {
            for (i, event) in access_trace.iter().enumerate() {
                if event.effect_ordinal_in_tx != i as u32 {
                    return Err(TabulaError::ConsistencyError(format!(
                        "invalid E-Trace identity for tx {} at time {}: expected effect ordinal {}, got {}",
                        tx_idx, event.time, i, event.effect_ordinal_in_tx
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Check consistency and return a typed status.
pub fn check_consistency_status(
    events: &[AccessEvent],
    read_set_old: &[(CellKey, Option<PortableValue>)],
    txs: &[TxResult],
) -> ExecutionConsistencyStatus {
    match check_consistency(events, read_set_old, txs) {
        Ok(()) => ExecutionConsistencyStatus::Passed,
        Err(e) => ExecutionConsistencyStatus::Failed {
            reason: e.to_string(),
        },
    }
}
