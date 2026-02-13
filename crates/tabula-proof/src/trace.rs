//! Witness data types for the proof trace.
//!
//! These types represent the structured witness data produced by `WitnessGenerator`:
//! per-column init/access rows, merge traces, column metadata, and the full batch witness.

use p3_baby_bear::BabyBear;

use tabula_commitment::{ColumnMeta, ColumnState, FieldHasher, MergeTrace, NativeDigest};
use tabula_core::event::{LogicalTime, TxOutcome};
use tabula_core::types::{CellKey, ColId, TableId, ValueType};

/// An init row seeding base-state values into the sorted memory table.
///
/// One per unique `(t,c,r)` read from committed state in the batch.
/// Timestamp is implicitly `τ = 0` (not stored).
#[derive(Clone, Debug)]
pub struct InitRow {
    /// The cell address.
    pub key: CellKey,
    /// Tier 1 ComEnc value (w(T) field elements). Canonical zero if null.
    pub value_fes: Vec<BabyBear>,
    /// Whether the cell was absent in base state.
    pub val_is_null: bool,
}

/// An access row from the execution trace (read or write).
#[derive(Clone, Debug)]
pub struct AccessRow {
    /// The cell address.
    pub key: CellKey,
    /// Logical time of this access (`τ = clk + 1`).
    pub time: LogicalTime,
    /// Whether this is a write (`true`) or read (`false`).
    pub is_write: bool,
    /// Tier 1 ComEnc value (w(T) field elements). Canonical zero if null.
    pub value_fes: Vec<BabyBear>,
    /// Whether the value is null.
    pub val_is_null: bool,
    /// Transaction index within the batch.
    pub tx_index: u32,
}

/// Complete witness data for a single `(table, col)`.
pub struct ColumnWitness<H: FieldHasher> {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// The column's value type (from schema).
    pub value_type: ValueType,
    /// Init rows from base state, sorted by row key.
    pub init_rows: Vec<InitRow>,
    /// Access rows from execution, in event order.
    pub access_rows: Vec<AccessRow>,
    /// Column state before the batch.
    pub old_state: ColumnState<H>,
    /// Column state after the batch.
    pub new_state: ColumnState<H>,
    /// SSMC merge trace (None for SMT columns).
    pub merge_trace: Option<MergeTrace>,
    /// Column metadata for the state-root transition.
    pub meta: ColumnMeta,
}

impl<H: FieldHasher> Clone for ColumnWitness<H> {
    fn clone(&self) -> Self {
        Self {
            table: self.table,
            col: self.col,
            value_type: self.value_type,
            init_rows: self.init_rows.clone(),
            access_rows: self.access_rows.clone(),
            old_state: self.old_state.clone(),
            new_state: self.new_state.clone(),
            merge_trace: self.merge_trace.clone(),
            meta: self.meta.clone(),
        }
    }
}

impl<H: FieldHasher> core::fmt::Debug for ColumnWitness<H> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ColumnWitness")
            .field("table", &self.table)
            .field("col", &self.col)
            .field("value_type", &self.value_type)
            .field("init_rows", &self.init_rows.len())
            .field("access_rows", &self.access_rows.len())
            .field("merge_trace", &self.merge_trace.is_some())
            .field("meta", &self.meta)
            .finish()
    }
}

/// The full batch witness: everything needed to build AIR traces.
pub struct BatchWitness<H: FieldHasher> {
    /// Per-column witness data.
    pub columns: Vec<ColumnWitness<H>>,
    /// State root before the batch.
    pub old_state_root: NativeDigest,
    /// State root after the batch.
    pub new_state_root: NativeDigest,
    /// Per-transaction outcomes.
    pub tx_outcomes: Vec<TxOutcome>,
}

impl<H: FieldHasher> Clone for BatchWitness<H> {
    fn clone(&self) -> Self {
        Self {
            columns: self.columns.clone(),
            old_state_root: self.old_state_root,
            new_state_root: self.new_state_root,
            tx_outcomes: self.tx_outcomes.clone(),
        }
    }
}

impl<H: FieldHasher> core::fmt::Debug for BatchWitness<H> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BatchWitness")
            .field("columns", &self.columns.len())
            .field("old_state_root", &self.old_state_root)
            .field("new_state_root", &self.new_state_root)
            .field("tx_outcomes", &self.tx_outcomes.len())
            .finish()
    }
}
