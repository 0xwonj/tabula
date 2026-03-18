//! Column layout for the SSMC property AIR.

use tabula_gadgets::{KeyRangeChecked, OrderingRangeChecked};
use tabula_stark::air::columns::num_cols;

/// Non-strict `lhs <= rhs` witness.
///
/// Uses either direct equality (`is_eq = 1`) or strict inequality
/// (`is_eq = 0` + `lt` witness).
#[repr(C)]
#[derive(Clone, Debug)]
pub struct LessOrEqChecked<T> {
    /// 1 iff `lhs == rhs`.
    pub is_eq: T,
    /// Strict inequality witness used when `is_eq = 0`.
    pub lt: OrderingRangeChecked<T>,
}

/// Column layout for the SSMC property AIR.
///
/// One row per `PropertyRead` query targeting one SSMC-backed column.
#[repr(C)]
pub struct SsmcPropertyCols<T, const W: usize> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,

    /// Query selectors.
    /// 1 iff the query kind is `Successor`.
    pub query_is_successor: T,
    /// 1 iff the query kind is `Predecessor`.
    pub query_is_predecessor: T,
    /// Canonical query kind ordinal.
    pub query_type: T,
    /// First canonical query operand.
    pub query_arg0: KeyRangeChecked<T>,
    /// Second canonical query operand.
    pub query_arg1: KeyRangeChecked<T>,

    /// Claimed execution result.
    pub result_val: [T; W],
    /// Claimed result row key.
    pub result_key: KeyRangeChecked<T>,
    /// 1 iff the claimed result is null.
    pub result_is_null: T,

    /// Empty-column witness vs anchored old-entry witness.
    pub uses_empty_old: T,
    /// 1 iff this row uses an old-entry anchor witness.
    pub uses_anchor: T,

    /// Anchored old entry and its local adjacency metadata.
    pub anchor_key: KeyRangeChecked<T>,
    /// Anchored old entry value.
    pub anchor_val: [T; W],
    /// 1 iff the anchor has a previous old entry.
    pub has_prev_old: T,
    /// Previous old-entry key when `has_prev_old = 1`.
    pub prev_old_key: KeyRangeChecked<T>,
    /// 1 iff the anchor is the last old entry.
    pub is_last_old: T,
    /// Next old-entry key when the anchor is not last.
    pub next_old_key: KeyRangeChecked<T>,

    /// Strict comparisons used by successor/predecessor proofs.
    pub query_lt_anchor: OrderingRangeChecked<T>,
    /// Strict comparison witness for `anchor_key < query_arg0`.
    pub anchor_lt_query: OrderingRangeChecked<T>,

    /// Non-strict comparisons used by adjacency/null proofs.
    pub prev_le_query: LessOrEqChecked<T>,
    /// Non-strict comparison witness for `anchor_key <= query_arg0`.
    pub anchor_le_query: LessOrEqChecked<T>,
    /// Non-strict comparison witness for `query_arg0 <= next_old_key`.
    pub query_le_next: LessOrEqChecked<T>,
    /// Non-strict comparison witness for `query_arg0 <= anchor_key`.
    pub query_le_anchor: LessOrEqChecked<T>,
}

/// Width of the SSMC property trace.
pub const fn ssmc_property_width<const W: usize>() -> usize {
    num_cols::<SsmcPropertyCols<u8, W>, u8>()
}

/// Standard width at `W=3`.
pub const SSMC_PROPERTY_STANDARD_WIDTH: usize = ssmc_property_width::<3>();
const _: () = assert!(SSMC_PROPERTY_STANDARD_WIDTH > 0);
