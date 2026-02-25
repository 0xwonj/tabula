//! Name-keyed trace storage for chip-agnostic proof pipelines.
//!
//! [`TraceMap`] replaces the hard-coded `AllTraceBundle<W>` struct, allowing
//! generic prover/verifier to iterate over chips without knowing their types.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;

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

/// Name-keyed map of chip traces.
///
/// Chips are identified by their [`ChipSpec::chip_name()`] string.
/// The prover iterates `CS::all_chips()` and looks up each chip's trace here.
#[derive(Debug, Clone)]
pub struct TraceMap {
    entries: BTreeMap<String, TraceEntry>,
}

impl TraceMap {
    /// Create an empty trace map.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Insert a main trace for a chip.
    pub fn insert(&mut self, name: &str, main: RowMajorMatrix<BabyBear>) {
        self.entries.insert(
            name.to_string(),
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
        name: &str,
        main: RowMajorMatrix<BabyBear>,
        preprocessed: RowMajorMatrix<BabyBear>,
    ) {
        self.entries.insert(
            name.to_string(),
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
    /// Debug-asserts that `name` exists in the map. In release builds,
    /// silently does nothing if the chip is absent.
    pub fn set_public_values(&mut self, name: &str, pvs: Vec<BabyBear>) {
        debug_assert!(
            self.entries.contains_key(name),
            "set_public_values: chip '{name}' not found in TraceMap"
        );
        if let Some(entry) = self.entries.get_mut(name) {
            entry.public_values = pvs;
        }
    }

    /// Look up a chip's trace entry.
    pub fn get(&self, name: &str) -> Option<&TraceEntry> {
        self.entries.get(name)
    }

    /// Insert a complete [`TraceEntry`] for a chip.
    pub fn insert_entry(&mut self, name: &str, entry: TraceEntry) {
        self.entries.insert(name.to_string(), entry);
    }

    /// All chip names present in the map, in sorted order.
    pub fn chip_names(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for TraceMap {
    fn default() -> Self {
        Self::new()
    }
}
