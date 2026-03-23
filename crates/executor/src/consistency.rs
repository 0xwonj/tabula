//! Key-local RAM consistency checks.
//!
//! The canonical input is the typed [`ExecutionJournal`](crate::journal::ExecutionJournal).
//! Portable-view helpers remain only as thin wrappers for tests.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{
    AccessEvent, CellKey, ExecutionConsistencyStatus, OpKind, PortableValue, TxResult,
};

use crate::journal::{
    ExecutionJournal, FailedAccessObservation, TxExecutionOutcome, TypedAccessEffect,
};

/// Check journal consistency against last-write semantics.
pub fn check_journal_consistency(journal: &ExecutionJournal) -> Result<(), TabulaError> {
    check_journal_etrace_identity(journal)?;

    let initial: BTreeMap<CellKey, Option<_>> = journal
        .state_summary
        .read_set_old
        .iter()
        .map(|entry| (entry.key, entry.value.clone()))
        .collect();

    let mut by_key: BTreeMap<CellKey, Vec<&TypedAccessEffect>> = BTreeMap::new();
    for effect in journal.successful_access_effects() {
        by_key.entry(effect.key).or_default().push(effect);
    }

    for (key, key_events) in &by_key {
        debug_assert!(
            key_events
                .windows(2)
                .all(|window| window[0].logical_time <= window[1].logical_time),
            "events for key {key:?} are not in time order"
        );

        let mut current_opt = initial.get(key).cloned().unwrap_or(None);
        for effect in key_events {
            match effect.op {
                OpKind::Write => current_opt = effect.value.clone(),
                OpKind::Read => {
                    if effect.value != current_opt {
                        return Err(TabulaError::ConsistencyError(format!(
                            "stale read at key {:?} time {}: expected {:?}, got {:?}",
                            effect.key, effect.logical_time, current_opt, effect.value,
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate canonical E-Trace identity constraints for the journal.
pub fn check_journal_etrace_identity(journal: &ExecutionJournal) -> Result<(), TabulaError> {
    for record in &journal.txs {
        match record {
            TxExecutionOutcome::Success(shard) => {
                check_success_etrace_identity(record.tx_index(), &shard.access_effects)?;
            }
            TxExecutionOutcome::Failed(failure) => {
                check_failed_diagnostic_identity(record.tx_index(), &failure.partial_accesses)?;
            }
        }
    }
    Ok(())
}

fn check_success_etrace_identity(
    tx_index: u32,
    effects: &[TypedAccessEffect],
) -> Result<(), TabulaError> {
    for (idx, effect) in effects.iter().enumerate() {
        if effect.effect_ordinal_in_tx != idx as u32 {
            return Err(TabulaError::ConsistencyError(format!(
                "invalid E-Trace identity for tx {} at time {}: expected effect ordinal {}, got {}",
                tx_index, effect.logical_time, idx, effect.effect_ordinal_in_tx,
            )));
        }
    }
    Ok(())
}

fn check_failed_diagnostic_identity(
    tx_index: u32,
    effects: &[FailedAccessObservation],
) -> Result<(), TabulaError> {
    let mut last_attempt_time = None;
    for (idx, effect) in effects.iter().enumerate() {
        if effect.effect_ordinal_in_tx != idx as u32 {
            return Err(TabulaError::ConsistencyError(format!(
                "invalid failed diagnostic identity for tx {} at attempt time {}: expected effect ordinal {}, got {}",
                tx_index, effect.attempt_time, idx, effect.effect_ordinal_in_tx,
            )));
        }
        if let Some(previous) = last_attempt_time
            && previous > effect.attempt_time
        {
            return Err(TabulaError::ConsistencyError(format!(
                "failed diagnostic attempt time regressed for tx {}: {} -> {}",
                tx_index, previous, effect.attempt_time,
            )));
        }
        last_attempt_time = Some(effect.attempt_time);
    }
    Ok(())
}

/// Typed status wrapper over journal consistency.
#[must_use]
pub fn check_journal_consistency_status(journal: &ExecutionJournal) -> ExecutionConsistencyStatus {
    match check_journal_consistency(journal) {
        Ok(()) => ExecutionConsistencyStatus::Passed,
        Err(error) => ExecutionConsistencyStatus::Failed {
            reason: error.to_string(),
        },
    }
}

/// Legacy portable-view consistency checker kept as a thin wrapper for tests.
pub fn check_consistency(
    events: &[AccessEvent],
    read_set_old: &[(CellKey, Option<PortableValue>)],
    txs: &[TxResult],
) -> Result<(), TabulaError> {
    check_etrace_identity(txs)?;

    let initial: BTreeMap<CellKey, Option<PortableValue>> = read_set_old.iter().cloned().collect();
    let mut by_key: BTreeMap<CellKey, Vec<&AccessEvent>> = BTreeMap::new();
    for event in events {
        by_key.entry(event.key).or_default().push(event);
    }

    fn event_to_opt(event: &AccessEvent) -> Option<PortableValue> {
        if event.val_is_null {
            None
        } else {
            Some(event.value.clone())
        }
    }

    for (key, key_events) in &by_key {
        debug_assert!(
            key_events
                .windows(2)
                .all(|window| window[0].time <= window[1].time),
            "events for key {key:?} are not in time order"
        );

        let mut current_opt = initial.get(key).cloned().unwrap_or(None);
        for event in key_events {
            match event.op {
                OpKind::Write => current_opt = event_to_opt(event),
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

/// Legacy portable-view E-Trace identity helper kept for tests.
pub fn check_etrace_identity(txs: &[TxResult]) -> Result<(), TabulaError> {
    for (tx_idx, tx) in txs.iter().enumerate() {
        let effects = match tx {
            TxResult::Success { access_trace, .. } => access_trace,
            TxResult::Failed { partial_events, .. } => partial_events,
        };
        for (idx, event) in effects.iter().enumerate() {
            if event.effect_ordinal_in_tx != idx as u32 {
                return Err(TabulaError::ConsistencyError(format!(
                    "invalid E-Trace identity for tx {} at time {}: expected effect ordinal {}, got {}",
                    tx_idx, event.time, idx, event.effect_ordinal_in_tx
                )));
            }
        }
    }
    Ok(())
}

/// Legacy portable-view status wrapper kept for tests.
#[must_use]
pub fn check_consistency_status(
    events: &[AccessEvent],
    read_set_old: &[(CellKey, Option<PortableValue>)],
    txs: &[TxResult],
) -> ExecutionConsistencyStatus {
    match check_consistency(events, read_set_old, txs) {
        Ok(()) => ExecutionConsistencyStatus::Passed,
        Err(error) => ExecutionConsistencyStatus::Failed {
            reason: error.to_string(),
        },
    }
}
