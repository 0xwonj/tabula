//! Witness data types for the proof trace.
//!
//! These types represent the structured witness data produced by `WitnessGenerator`:
//! per-column init/access rows, merge traces, column metadata, and the full batch witness.

use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnMeta, ColumnState, FieldHasher, MergeTrace, NativeDigest};
use tabula_core::{CellKey, ColId, LogicalTime, TableId, TxResult, ValueType};

use super::route::KeyRoute;

/// An init row seeding base-state values into the sorted memory table.
///
/// One per unique `(t,c,r)` read from committed state in the batch.
/// Timestamp is implicitly `τ = 0` (not stored).
#[derive(Clone, Debug, PartialEq)]
pub struct InitRow {
    /// The cell address.
    pub key: CellKey,
    /// Tier 1 ComEnc value (w(T) field elements). Canonical zero if null.
    pub value_fes: Vec<KoalaBear>,
    /// Whether the cell was absent in base state.
    pub val_is_null: bool,
}

/// An access row from the execution trace (read or write).
#[derive(Clone, Debug, PartialEq)]
pub struct AccessRow {
    /// The cell address.
    pub key: CellKey,
    /// Logical time of this access (`τ = clk + 1`).
    pub time: LogicalTime,
    /// Whether this is a write (`true`) or read (`false`).
    pub is_write: bool,
    /// Tier 1 ComEnc value (w(T) field elements). Canonical zero if null.
    pub value_fes: Vec<KoalaBear>,
    /// Whether the value is null.
    pub val_is_null: bool,
    /// Transaction index within the batch.
    pub tx_index: u32,
    /// Effect ordinal within the transaction.
    pub effect_ordinal_in_tx: u32,
}

/// Complete witness data for a single `(table, col)`.
#[derive(Clone)]
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
#[derive(Clone)]
pub struct BatchWitness<H: FieldHasher> {
    /// Per-column witness data.
    pub columns: Vec<ColumnWitness<H>>,
    /// Flat list of column metadata, sorted by `(table, col)`.
    /// Ready for `generate_column_meta_trace()`.
    pub column_metas: Vec<ColumnMeta>,
    /// State root before the batch.
    pub old_state_root: NativeDigest,
    /// State root after the batch.
    pub new_state_root: NativeDigest,
    /// Per-transaction results.
    pub tx_results: Vec<TxResult>,
    /// Per-key proof path routing (ReadOnly / ShortRun / SortedMemory).
    /// Determines which memory-layer chip handles each key.
    pub key_routes: BTreeMap<CellKey, KeyRoute>,
}

impl<H: FieldHasher> core::fmt::Debug for BatchWitness<H> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BatchWitness")
            .field("columns", &self.columns.len())
            .field("column_metas", &self.column_metas.len())
            .field("old_state_root", &self.old_state_root)
            .field("new_state_root", &self.new_state_root)
            .field("tx_results", &self.tx_results.len())
            .field("key_routes", &self.key_routes.len())
            .finish()
    }
}
