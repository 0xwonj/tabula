//! Shared execution-row types for runtime-owned column proof preparation.

use p3_koala_bear::KoalaBear;

use tabula_core::{CellKey, LogicalTime};

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
