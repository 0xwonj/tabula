//! Column layout for the PropertyVerifier AIR.

use tabula_stark::air::columns::num_cols;

/// Column layout for the PropertyVerifier AIR.
///
/// One row per PropertyRead query on this column. Generic over `W` (value width).
/// Width at W=3: 11 columns.
#[repr(C)]
pub struct PropertyVerifierCols<T, const W: usize> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier (constant across all real rows).
    pub table_id: T,
    /// Column identifier (constant across all real rows).
    pub col_id: T,
    /// Query type ordinal (0=Minimum, 1=Maximum, 2=Successor, etc.).
    pub query_type: T,
    /// Result value from the property query (W field elements).
    pub result_val: [T; W],
    /// Result key from the property query (W field elements).
    pub result_key: [T; W],
    /// Whether the result is null (no matching entry found).
    pub is_null: T,
}

/// Width of the PropertyVerifier trace (number of columns).
pub const fn property_verifier_width<const W: usize>() -> usize {
    num_cols::<PropertyVerifierCols<u8, W>, u8>()
}

/// Standard width at W=3 (the default value width).
pub const PROPERTY_VERIFIER_STANDARD_WIDTH: usize = property_verifier_width::<3>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_width_is_11() {
        assert_eq!(PROPERTY_VERIFIER_STANDARD_WIDTH, 11);
    }
}
