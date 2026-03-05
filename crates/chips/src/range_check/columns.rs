//! Column layout for the range check table.

use tabula_stark::air::columns::num_cols;

/// Range check table size: 2^16 = 65536 rows.
///
/// 16 bits is chosen to split 30-bit u64 limbs into two 15-bit halves,
/// each fitting within [0, 2^16). The 4-bit top limb also fits trivially.
pub const RANGE_CHECK_SIZE: usize = 1 << 16;

/// Column layout for the range check table.
///
/// - `value`: preprocessed (0..2^16)
/// - `multiplicity`: main trace (how many times this value is looked up)
#[repr(C)]
pub struct RangeCheckCols<T> {
    /// The value being range-checked (preprocessed: 0, 1, ..., 2^16-1).
    pub value: T,
    /// Number of times this value is looked up (main trace).
    pub multiplicity: T,
}

/// Width of the RangeCheck trace.
pub const RANGE_CHECK_WIDTH: usize = num_cols::<RangeCheckCols<u8>, u8>();
