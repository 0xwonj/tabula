//! TraceContributor trait, TracePhase, and WitnessStore for generic orchestration.
//!
//! Each chip implements [`TraceContributor`] to generate its own trace from
//! a [`WitnessStore`] containing typed inputs. The orchestrator dispatches
//! chips by [`TracePhase`] order without per-chip hardcoding.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use tabula_core::error::TabulaError;

use super::trace_map::TraceMap;
use crate::chips::ChipSpec;

/// Execution phase for trace generation.
///
/// Chips are dispatched in phase order. Between Memory and Dependent phases,
/// the orchestrator collects interaction data (C5 Poseidon inputs, C8 range
/// check multiplicities) from Phase 0+1 chip traces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TracePhase {
    /// Chips whose traces are independent: Execution, StaticTable, SmtColPath, SmtTablePath.
    Independent = 0,
    /// Memory-layer chips built from witness data: InterTxOrder, StateColumn, ColumnMeta.
    Memory = 1,
    /// Chips consuming interaction data from Phase 0+1: Poseidon, RangeCheck.
    Dependent = 2,
}

/// Trait for chips that can generate their own trace from a [`WitnessStore`].
///
/// The orchestrator calls `contribute()` for each chip in phase order.
/// Each chip pulls its inputs from the store and inserts its trace into the map.
pub trait TraceContributor: ChipSpec {
    /// Which phase this chip belongs to.
    fn phase(&self) -> TracePhase;

    /// Generate this chip's trace from the store and insert it into the map.
    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError>;
}

/// Type-safe key for [`WitnessStore`] entries.
///
/// Combines a `TypeId` (for downcasting safety) with a `&'static str` label
/// (for disambiguation when multiple values of the same type exist).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WitnessKey {
    type_id: TypeId,
    label: &'static str,
}

impl WitnessKey {
    /// Create a key for type `T` with a label.
    pub fn of<T: 'static>(label: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            label,
        }
    }
}

/// Type-safe data store for inter-chip data exchange during trace generation.
///
/// The caller populates the store with witness data before calling the
/// generic orchestrator. Chips pull their inputs from the store via typed keys.
pub struct WitnessStore {
    entries: HashMap<WitnessKey, Box<dyn Any + Send + Sync>>,
}

impl WitnessStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a typed value under a key.
    pub fn put<T: Send + Sync + 'static>(&mut self, label: &'static str, value: T) {
        let key = WitnessKey::of::<T>(label);
        self.entries.insert(key, Box::new(value));
    }

    /// Get a reference to a typed value by key.
    ///
    /// Returns an error if the key is missing or the type doesn't match.
    pub fn get<T: 'static>(&self, label: &'static str) -> Result<&T, TabulaError> {
        let key = WitnessKey::of::<T>(label);
        self.entries
            .get(&key)
            .and_then(|v| v.downcast_ref::<T>())
            .ok_or_else(|| TabulaError::ProofError {
                phase: "witness_store",
                detail: format!("missing or type-mismatched entry for key '{}'", label),
            })
    }

    /// Check whether a key exists in the store.
    pub fn contains<T: 'static>(&self, label: &'static str) -> bool {
        let key = WitnessKey::of::<T>(label);
        self.entries.contains_key(&key)
    }
}

impl Default for WitnessStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Well-known labels for [`WitnessStore`] entries used by core chips.
///
/// Each label uniquely identifies a data payload that the orchestrator
/// or a chip's [`TraceContributor::contribute`] reads from the store.
pub mod witness_labels {
    /// `Vec<InstructionRecord>` — execution instruction trace input.
    pub const EXECUTION_RECORDS: &str = "execution_records";
    /// `Vec<InterTxOrderRow>` — pre-sorted inter-tx ordering rows.
    pub const INTER_TX_ROWS: &str = "inter_tx_rows";
    /// `Vec<StateColumnRow>` — pre-sorted state column rows.
    pub const STATE_ROWS: &str = "state_rows";
    /// `ColumnMetaInput` — column metadata + empty-read counts.
    pub const COLUMN_META_INPUT: &str = "column_meta_input";
    /// `Vec<StaticTableRow>` — static table lookup rows.
    pub const STATIC_TABLE_ROWS: &str = "static_table_rows";
    /// `Vec<SmtPathWitness>` — SMT column-level path witnesses.
    pub const SMT_COL_PATHS: &str = "smt_col_paths";
    /// `Vec<SmtTablePathWitness>` — SMT table-level path witnesses.
    pub const SMT_TABLE_PATHS: &str = "smt_table_paths";
    /// `Vec<BabyBear>` — SmtTablePath public values (old/new root).
    pub const SMT_TABLE_PVS: &str = "smt_table_pvs";
    /// `Vec<[BabyBear; 16]>` — Poseidon permutation inputs (collected from Phase 0+1).
    pub const POSEIDON_INPUTS: &str = "poseidon_inputs";
    /// `Box<[u32; RANGE_CHECK_SIZE]>` — range check multiplicities (collected from Phase 0+1).
    pub const RANGE_CHECK_MULTS: &str = "range_check_mults";
}
