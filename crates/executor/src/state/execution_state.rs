//! State management for the execution overlay.
//!
//! Handles the write buffer, read cache, and undo log for
//! checkpoint/rollback. Does NOT record events.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CommittedCellKey, TypeId};
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::surface::{TypedStateSnapshot, TypedStateWrite};

/// An undo-log entry for reverting a single mutation.
pub(crate) enum UndoEntry {
    /// A write_buffer mutation: key had `prev` value (None = key was absent in buffer).
    Write {
        key: CommittedCellKey,
        prev: Option<TypedCellValue>,
    },
    /// A read_cache fill: key was absent before this tx.
    ReadCacheFill { key: CommittedCellKey },
}

/// Checkpoint for execution state (undo log position only).
pub(crate) struct StateCheckpoint {
    pub(crate) undo_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypedCellValue {
    pub(crate) type_id: TypeId,
    pub(crate) value: Option<TypedValue>,
}

/// State management sub-component of `Overlay`.
///
/// Handles the write buffer, read cache, and undo log for
/// checkpoint/rollback. Does NOT record events.
pub(crate) struct ExecutionState<'a, S: StateView> {
    pub(crate) snapshot: &'a S,
    pub(crate) write_buffer: BTreeMap<CommittedCellKey, TypedCellValue>,
    pub(crate) read_cache: BTreeMap<CommittedCellKey, TypedCellValue>,
    pub(crate) undo_log: Vec<UndoEntry>,
    pub(crate) checkpoints: Vec<StateCheckpoint>,
}

impl<'a, S: StateView> ExecutionState<'a, S> {
    pub(crate) fn new(snapshot: &'a S) -> Self {
        Self {
            snapshot,
            write_buffer: BTreeMap::new(),
            read_cache: BTreeMap::new(),
            undo_log: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Check the write buffer for a key. Returns `None` if not in buffer.
    pub(crate) fn read_from_buffer(&self, key: &CommittedCellKey) -> Option<&TypedCellValue> {
        self.write_buffer.get(key)
    }

    /// Check the read cache for a key. Returns `None` if not cached.
    pub(crate) fn read_from_cache(&self, key: &CommittedCellKey) -> Option<&TypedCellValue> {
        self.read_cache.get(key)
    }

    /// Read from the snapshot, filling the read cache and undo log.
    pub(crate) fn read_from_snapshot(
        &mut self,
        key: &CommittedCellKey,
        type_id: TypeId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<TypedCellValue, TabulaError> {
        let opt = self
            .snapshot
            .read(key)?
            .map(|value| {
                if value.type_id() != type_id {
                    return Err(TabulaError::TypeMismatch {
                        expected: format!("type_id {}", type_id.0),
                        actual: format!("type_id {}", value.type_id().0),
                    });
                }
                type_runtimes.decode_portable(&value)
            })
            .transpose()?;
        let cell_value = TypedCellValue {
            type_id,
            value: opt,
        };
        self.read_cache.insert(key.clone(), cell_value.clone());
        if !self.checkpoints.is_empty() {
            self.undo_log
                .push(UndoEntry::ReadCacheFill { key: key.clone() });
        }
        Ok(cell_value)
    }

    /// Buffer a write, recording the previous value in the undo log.
    pub(crate) fn write_buffered(
        &mut self,
        key: &CommittedCellKey,
        type_id: TypeId,
        value: Option<TypedValue>,
    ) {
        if !self.checkpoints.is_empty() {
            let prev = self.write_buffer.get(key).cloned();
            self.undo_log.push(UndoEntry::Write {
                key: key.clone(),
                prev,
            });
        }
        self.write_buffer
            .insert(key.clone(), TypedCellValue { type_id, value });
    }

    pub(crate) fn checkpoint(&mut self) {
        self.checkpoints.push(StateCheckpoint {
            undo_len: self.undo_log.len(),
        });
    }

    pub(crate) fn rollback(&mut self) -> Option<()> {
        let cp = self.checkpoints.pop()?;
        while self.undo_log.len() > cp.undo_len {
            match self.undo_log.pop().unwrap() {
                UndoEntry::Write { key, prev } => match prev {
                    Some(opt_v) => {
                        self.write_buffer.insert(key, opt_v);
                    }
                    None => {
                        self.write_buffer.remove(&key);
                    }
                },
                UndoEntry::ReadCacheFill { key } => {
                    self.read_cache.remove(&key);
                }
            }
        }
        Some(())
    }

    pub(crate) fn discard_checkpoint(&mut self) {
        self.checkpoints.pop();
        if self.checkpoints.is_empty() {
            self.undo_log.clear();
        }
    }

    /// Consume into (read_set_old, write_set_final).
    pub(crate) fn into_sets(
        self,
        _type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<(Vec<TypedStateSnapshot>, Vec<TypedStateWrite>), TabulaError> {
        let read_set_old = self
            .read_cache
            .into_iter()
            .map(|(key, value)| TypedStateSnapshot {
                key,
                type_id: value.type_id,
                value: value.value,
            })
            .collect();
        let write_set_final = self
            .write_buffer
            .into_iter()
            .map(|(key, value)| TypedStateWrite {
                key,
                type_id: value.type_id,
                value: value.value,
            })
            .collect();
        Ok((read_set_old, write_set_final))
    }
}
