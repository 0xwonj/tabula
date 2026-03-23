//! Event recording for the execution overlay.
//!
//! Handles the execution event trace, logical time, and tx index.

use tabula_core::{AccessEvent, CellKey, LogicalTime, OpKind, TypeId};
use tabula_types::{TypeRuntimeRegistry, TypedValue};

/// Checkpoint for the trace recorder.
pub(crate) struct RecorderCheckpoint {
    pub(crate) events_len: usize,
    pub(crate) time: LogicalTime,
    pub(crate) tx_index: u32,
    pub(crate) effect_ordinal_in_tx: u32,
}

/// Event recording sub-component of `Overlay`.
///
/// Handles the execution event trace, logical time, and tx index.
/// Accessible as `pub(crate)` for future ok-gating support, where
/// events must be preserved even when state is rolled back.
pub(crate) struct TraceRecorder {
    events: Vec<AccessEvent>,
    time: LogicalTime,
    current_tx_index: u32,
    current_effect_ordinal_in_tx: u32,
    checkpoints: Vec<RecorderCheckpoint>,
}

impl TraceRecorder {
    pub(crate) fn new() -> Self {
        Self {
            events: Vec::new(),
            time: 0,
            current_tx_index: 0,
            current_effect_ordinal_in_tx: 0,
            checkpoints: Vec::new(),
        }
    }

    /// Record an execution event and advance the logical clock.
    pub(crate) fn record_event(
        &mut self,
        key: &CellKey,
        op: OpKind,
        opt_value: &Option<TypedValue>,
        col_type: TypeId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<(), tabula_core::error::TabulaError> {
        let (value, val_is_null) = match opt_value {
            Some(v) => (type_runtimes.encode_typed(v)?, false),
            None => (
                type_runtimes.encode_typed(&type_runtimes.zero_of(col_type)?)?,
                true,
            ),
        };
        self.events.push(AccessEvent {
            key: *key,
            op,
            value,
            val_is_null,
            time: self.time,
            effect_ordinal_in_tx: self.current_effect_ordinal_in_tx,
        });
        self.current_effect_ordinal_in_tx += 1;
        self.time += 1;
        Ok(())
    }

    pub(crate) fn checkpoint(&mut self) {
        self.checkpoints.push(RecorderCheckpoint {
            events_len: self.events.len(),
            time: self.time,
            tx_index: self.current_tx_index,
            effect_ordinal_in_tx: self.current_effect_ordinal_in_tx,
        });
    }

    pub(crate) fn rollback(&mut self) -> Option<()> {
        let cp = self.checkpoints.pop()?;
        self.events.truncate(cp.events_len);
        self.time = cp.time;
        self.current_tx_index = cp.tx_index;
        self.current_effect_ordinal_in_tx = cp.effect_ordinal_in_tx;
        Some(())
    }

    pub(crate) fn discard_checkpoint(&mut self) {
        self.checkpoints.pop();
    }

    pub(crate) fn time(&self) -> LogicalTime {
        self.time
    }

    pub(crate) fn set_tx_index(&mut self, idx: u32) {
        self.current_tx_index = idx;
        self.current_effect_ordinal_in_tx = 0;
    }

    pub(crate) fn events_len(&self) -> usize {
        self.events.len()
    }

    pub(crate) fn events_since(&self, since: usize) -> Vec<AccessEvent> {
        self.events[since..].to_vec()
    }

    pub(crate) fn into_events(self) -> Vec<AccessEvent> {
        self.events
    }
}
