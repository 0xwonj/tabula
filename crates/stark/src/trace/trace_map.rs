//! Typed trace storage for chip-agnostic proof pipelines.
//!
//! [`TraceMap`] replaces the hard-coded `AllTraceBundle<W>` struct, allowing
//! generic prover/verifier to iterate over chips without knowing their types.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;

use crate::chips::ChipId;

/// One chip's trace data: main trace + optional preprocessed + public values.
#[derive(Debug, Clone)]
pub struct TraceEntry {
    /// Main trace matrix.
    pub main: RowMajorMatrix<BabyBear>,
    /// Preprocessed trace matrix (e.g. Poseidon round constants).
    pub preprocessed: Option<RowMajorMatrix<BabyBear>>,
    /// Public values for this chip (empty for most chips).
    pub public_values: Vec<BabyBear>,
}

impl TraceEntry {
    /// Create a trace entry with only a main trace (no preprocessed, no public values).
    pub fn main_only(main: RowMajorMatrix<BabyBear>) -> Self {
        Self {
            main,
            preprocessed: None,
            public_values: vec![],
        }
    }
}

/// Typed map of chip traces keyed by [`ChipId`].
///
/// The prover iterates `CS::all_chips()` and looks up each chip's trace here.
#[derive(Debug, Clone)]
pub struct TraceMap {
    entries: BTreeMap<ChipId, TraceEntry>,
}

impl TraceMap {
    /// Create an empty trace map.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Insert a main trace for a chip.
    pub fn insert(&mut self, id: ChipId, main: RowMajorMatrix<BabyBear>) {
        self.entries.insert(
            id,
            TraceEntry {
                main,
                preprocessed: None,
                public_values: vec![],
            },
        );
    }

    /// Insert a main trace with preprocessed data.
    pub fn insert_with_preprocessed(
        &mut self,
        id: ChipId,
        main: RowMajorMatrix<BabyBear>,
        preprocessed: RowMajorMatrix<BabyBear>,
    ) {
        self.entries.insert(
            id,
            TraceEntry {
                main,
                preprocessed: Some(preprocessed),
                public_values: vec![],
            },
        );
    }

    /// Set public values for a chip that has already been inserted.
    ///
    /// # Panics
    ///
    /// Debug-asserts that `id` exists in the map. In release builds,
    /// silently does nothing if the chip is absent.
    pub fn set_public_values(&mut self, id: ChipId, pvs: Vec<BabyBear>) {
        debug_assert!(
            self.entries.contains_key(&id),
            "set_public_values: chip '{id}' not found in TraceMap"
        );
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.public_values = pvs;
        }
    }

    /// Look up a chip's trace entry.
    pub fn get(&self, id: ChipId) -> Option<&TraceEntry> {
        self.entries.get(&id)
    }

    /// Insert a complete [`TraceEntry`] for a chip.
    pub fn insert_entry(&mut self, id: ChipId, entry: TraceEntry) {
        self.entries.insert(id, entry);
    }

    /// Remove and return a chip's trace entry, transferring ownership.
    pub fn remove(&mut self, id: ChipId) -> Option<TraceEntry> {
        self.entries.remove(&id)
    }

    /// All chip IDs present in the map, in sorted order.
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.entries.keys().copied().collect()
    }
}

impl Default for TraceMap {
    fn default() -> Self {
        Self::new()
    }
}
