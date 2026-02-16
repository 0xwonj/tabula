//! RangeCheckChip — preprocessed lookup table for range checks.
//!
//! Contains values `[0, 2^16)` with a multiplicity column for LogUp.
//! No AIR constraints needed — the table is preprocessed (fixed at setup time).
//! LogUp bus `InteractionKind::RangeCheck` wired in M9.
//!
//! Other chips decompose values into sub-limbs and send range-check requests:
//! - u64 limbs (30 bits) → two 15-bit halves → two lookups each in [0, 2^16)
//! - u64 top limb (4 bits) → single lookup in [0, 16) ⊂ [0, 2^16)
//! - StrictIneq gap limbs → same decomposition

use p3_air::{Air, AirBuilder, BaseAir};
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::{borrow_cols_mut, num_cols};

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

/// The RangeCheck AIR chip.
///
/// No constraints — the table is preprocessed. Soundness comes from the LogUp
/// argument: any chip sending a range-check request must have a matching entry
/// in this table. If the requested value is outside `[0, 2^16)`, no matching
/// row exists and the LogUp argument fails.
#[derive(Debug)]
pub struct RangeCheckChip;

impl<F> BaseAir<F> for RangeCheckChip {
    fn width(&self) -> usize {
        RANGE_CHECK_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for RangeCheckChip {
    fn eval(&self, _builder: &mut AB) {
        // No constraints. Soundness is via LogUp: if a chip sends a value
        // not in [0, 2^16), no matching preprocessed row exists and the
        // LogUp sum won't balance.
    }
}

/// Generate the preprocessed range check table.
///
/// Returns a `2^16`-row trace where row `i` has `value = i` and `multiplicity = 0`.
/// The multiplicity column is filled during the proving phase based on actual lookups.
pub fn generate_range_check_preprocessed() -> RowMajorMatrix<BabyBear> {
    let mut values = vec![BabyBear::ZERO; RANGE_CHECK_SIZE * RANGE_CHECK_WIDTH];

    for i in 0..RANGE_CHECK_SIZE {
        let offset = i * RANGE_CHECK_WIDTH;
        let row: &mut RangeCheckCols<BabyBear> =
            borrow_cols_mut(&mut values[offset..offset + RANGE_CHECK_WIDTH]);
        row.value = BabyBear::new(i as u32);
        row.multiplicity = BabyBear::ZERO; // filled during proving
    }

    RowMajorMatrix::new(values, RANGE_CHECK_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::columns::borrow_cols;
    use crate::air::debug::debug_check;
    use p3_matrix::Matrix;

    #[test]
    fn range_check_width_is_two() {
        assert_eq!(RANGE_CHECK_WIDTH, 2);
    }

    #[test]
    fn preprocessed_table_size() {
        let trace = generate_range_check_preprocessed();
        assert_eq!(trace.height(), RANGE_CHECK_SIZE);
        assert_eq!(trace.width, RANGE_CHECK_WIDTH);
    }

    #[test]
    fn preprocessed_table_values() {
        let trace = generate_range_check_preprocessed();
        // Spot check first, last, and middle
        let row_0: &RangeCheckCols<BabyBear> = borrow_cols(&trace.values[0..RANGE_CHECK_WIDTH]);
        assert_eq!(row_0.value, BabyBear::ZERO);

        let last = RANGE_CHECK_SIZE - 1;
        let offset = last * RANGE_CHECK_WIDTH;
        let row_last: &RangeCheckCols<BabyBear> =
            borrow_cols(&trace.values[offset..offset + RANGE_CHECK_WIDTH]);
        assert_eq!(row_last.value, BabyBear::new(last as u32));
    }

    #[test]
    fn preprocessed_table_multiplicities_zero() {
        let trace = generate_range_check_preprocessed();
        for i in 0..RANGE_CHECK_SIZE {
            let offset = i * RANGE_CHECK_WIDTH;
            let row: &RangeCheckCols<BabyBear> =
                borrow_cols(&trace.values[offset..offset + RANGE_CHECK_WIDTH]);
            assert_eq!(row.multiplicity, BabyBear::ZERO);
        }
    }

    #[test]
    fn range_check_chip_no_constraints() {
        // The chip has no constraints, so any trace should pass.
        let trace = generate_range_check_preprocessed();
        debug_check(&RangeCheckChip, &trace).expect("no constraints to fail");
    }
}
