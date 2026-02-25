//! Column layout for the StaticTable AIR.

use crate::air::columns::num_cols;
use crate::gadgets::U64Limbs;

/// Column layout for the StaticTable AIR.
///
/// `#[repr(C)]` ensures field order matches the flat trace slice.
/// `W` is the value width (3 for Standard).
#[repr(C)]
pub struct StaticTableCols<T, const W: usize> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,
    /// Row key (u64, 3 limbs).
    pub row_key: U64Limbs<T>,
    /// Value (W field elements).
    pub value: [T; W],
    /// Lookup multiplicity witness on C9.
    ///
    /// This allows one static row to satisfy multiple lookup sends from Execution.
    /// LogUp soundness enforces correctness of this free witness.
    pub lookup_mult_witness: T,
}

/// Compute the width of a StaticTableCols for a given W.
pub const fn static_table_width<const W: usize>() -> usize {
    num_cols::<StaticTableCols<u8, W>, u8>()
}

/// Width of the StaticTable trace for Standard value width (W=3).
pub const STATIC_TABLE_STANDARD_WIDTH: usize = static_table_width::<3>();
