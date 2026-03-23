//! State management for the execution overlay.
//!
//! Handles the write buffer, read cache, and undo log for
//! checkpoint/rollback. Does NOT record events.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CellKey, PortableValue};
use tabula_types::{TypeRuntimeRegistry, TypedValue};

/// An undo-log entry for reverting a single mutation.
pub(crate) enum UndoEntry {
    /// A write_buffer mutation: key had `prev` value (None = key was absent in buffer).
    Write {
        key: CellKey,
        prev: Option<Option<TypedValue>>,
    },
    /// A read_cache fill: key was absent before this tx.
    ReadCacheFill { key: CellKey },
}

/// Checkpoint for execution state (undo log position only).
pub(crate) struct StateCheckpoint {
    pub(crate) undo_len: usize,
}

pub(crate) type PortableCellEntries = Vec<(CellKey, Option<PortableValue>)>;

/// State management sub-component of `Overlay`.
///
/// Handles the write buffer, read cache, and undo log for
/// checkpoint/rollback. Does NOT record events.
pub(crate) struct ExecutionState<'a, S: StateView> {
    pub(crate) snapshot: &'a S,
    pub(crate) write_buffer: BTreeMap<CellKey, Option<TypedValue>>,
    pub(crate) read_cache: BTreeMap<CellKey, Option<TypedValue>>,
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
    pub(crate) fn read_from_buffer(&self, key: &CellKey) -> Option<&Option<TypedValue>> {
        self.write_buffer.get(key)
    }

    /// Check the read cache for a key. Returns `None` if not cached.
    pub(crate) fn read_from_cache(&self, key: &CellKey) -> Option<&Option<TypedValue>> {
        self.read_cache.get(key)
    }

    /// Read from the snapshot, filling the read cache and undo log.
    pub(crate) fn read_from_snapshot(
        &mut self,
        key: &CellKey,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Option<TypedValue>, TabulaError> {
        let opt = self
            .snapshot
            .read(key)?
            .map(|value| type_runtimes.decode_portable(&value))
            .transpose()?;
        self.read_cache.insert(*key, opt.clone());
        if !self.checkpoints.is_empty() {
            self.undo_log.push(UndoEntry::ReadCacheFill { key: *key });
        }
        Ok(opt)
    }

    /// Buffer a write, recording the previous value in the undo log.
    pub(crate) fn write_buffered(&mut self, key: &CellKey, value: Option<TypedValue>) {
        if !self.checkpoints.is_empty() {
            let prev = self.write_buffer.get(key).cloned();
            self.undo_log.push(UndoEntry::Write { key: *key, prev });
        }
        self.write_buffer.insert(*key, value);
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
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<(PortableCellEntries, PortableCellEntries), TabulaError> {
        let read_set_old = self
            .read_cache
            .into_iter()
            .map(|(key, value)| {
                let portable = value
                    .as_ref()
                    .map(|typed| type_runtimes.encode_typed(typed))
                    .transpose()?;
                Ok((key, portable))
            })
            .collect::<Result<Vec<_>, TabulaError>>()?;
        let write_set_final = self
            .write_buffer
            .into_iter()
            .map(|(key, value)| {
                let portable = value
                    .as_ref()
                    .map(|typed| type_runtimes.encode_typed(typed))
                    .transpose()?;
                Ok((key, portable))
            })
            .collect::<Result<Vec<_>, TabulaError>>()?;
        Ok((read_set_old, write_set_final))
    }
}
