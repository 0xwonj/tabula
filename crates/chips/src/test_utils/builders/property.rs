//! Test builders for SSMC property-chip traces.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::{Limb2Bits, LimbHalves, OrderingRangeChecked, StrictIneq};
use tabula_stark::air::columns::borrow_cols_mut;

use crate::execution::u64_to_limbs;
use crate::shards::property::columns::{LessOrEqChecked, SsmcPropertyCols, ssmc_property_width};

/// High-level row descriptor for SSMC property-chip tests.
#[derive(Debug, Clone)]
pub enum SsmcPropertyTestRow {
    /// Non-null successor result.
    Successor {
        /// Query key whose successor is requested.
        query_key: u64,
        /// Anchored successor key.
        anchor_key: u64,
        /// Previous old key, when the successor is not the first old row.
        prev_key: Option<u64>,
        /// Anchored successor value.
        result_val: Vec<KoalaBear>,
    },
    /// Non-null predecessor result.
    Predecessor {
        /// Query key whose predecessor is requested.
        query_key: u64,
        /// Anchored predecessor key.
        anchor_key: u64,
        /// Next old key, when the predecessor is not the last old row.
        next_key: Option<u64>,
        /// Anchored predecessor value.
        result_val: Vec<KoalaBear>,
    },
    /// Null successor result for an empty old column.
    EmptySuccessor {
        /// Query key whose successor is requested.
        query_key: u64,
    },
    /// Null predecessor result for an empty old column.
    EmptyPredecessor {
        /// Query key whose predecessor is requested.
        query_key: u64,
    },
}

fn zero_ordering() -> OrderingRangeChecked<KoalaBear> {
    OrderingRangeChecked {
        ineq: StrictIneq {
            diff0: KoalaBear::ZERO,
            diff1: KoalaBear::ZERO,
            diff2: KoalaBear::ZERO,
            borrow0: KoalaBear::ZERO,
            borrow1: KoalaBear::ZERO,
        },
        diff0_halves: LimbHalves {
            lo: KoalaBear::ZERO,
            hi: KoalaBear::ZERO,
        },
        diff1_halves: LimbHalves {
            lo: KoalaBear::ZERO,
            hi: KoalaBear::ZERO,
        },
        diff2_bits: Limb2Bits {
            b0: KoalaBear::ZERO,
            b1: KoalaBear::ZERO,
            b2: KoalaBear::ZERO,
            b3: KoalaBear::ZERO,
        },
    }
}

fn populate_leq(leq: &mut LessOrEqChecked<KoalaBear>, lhs: u64, rhs: u64) {
    if lhs == rhs {
        leq.is_eq = KoalaBear::ONE;
        leq.lt = zero_ordering();
    } else {
        leq.is_eq = KoalaBear::ZERO;
        leq.lt.populate(lhs, rhs);
    }
}

fn zero_value<const W: usize>() -> Vec<KoalaBear> {
    vec![KoalaBear::ZERO; W]
}

fn assign_value<const W: usize>(dst: &mut [KoalaBear; W], src: &[KoalaBear]) {
    for (out, value) in dst.iter_mut().zip(
        src.iter()
            .copied()
            .chain(core::iter::repeat(KoalaBear::ZERO)),
    ) {
        *out = value;
    }
}

fn fill_row<const W: usize>(
    cols: &mut SsmcPropertyCols<KoalaBear, W>,
    table_id: u32,
    col_id: u16,
    row: &SsmcPropertyTestRow,
) {
    cols.is_real = KoalaBear::ONE;
    cols.table_id = KoalaBear::new(table_id);
    cols.col_id = KoalaBear::new(u32::from(col_id));
    cols.query_lt_anchor = zero_ordering();
    cols.anchor_lt_query = zero_ordering();
    cols.prev_le_query = LessOrEqChecked {
        is_eq: KoalaBear::ZERO,
        lt: zero_ordering(),
    };
    cols.anchor_le_query = LessOrEqChecked {
        is_eq: KoalaBear::ZERO,
        lt: zero_ordering(),
    };
    cols.query_le_next = LessOrEqChecked {
        is_eq: KoalaBear::ZERO,
        lt: zero_ordering(),
    };
    cols.query_le_anchor = LessOrEqChecked {
        is_eq: KoalaBear::ZERO,
        lt: zero_ordering(),
    };

    match row {
        SsmcPropertyTestRow::Successor {
            query_key,
            anchor_key,
            prev_key,
            result_val,
        } => {
            cols.query_is_successor = KoalaBear::ONE;
            cols.query_type = KoalaBear::new(2);
            cols.query_arg0.populate(*query_key);
            assign_value(&mut cols.result_val, result_val);
            cols.result_key.populate(*anchor_key);
            cols.uses_anchor = KoalaBear::ONE;
            cols.anchor_key.populate(*anchor_key);
            assign_value(&mut cols.anchor_val, result_val);
            cols.query_lt_anchor.populate(*query_key, *anchor_key);
            if let Some(prev_key) = prev_key {
                cols.has_prev_old = KoalaBear::ONE;
                cols.prev_old_key.populate(*prev_key);
                populate_leq(&mut cols.prev_le_query, *prev_key, *query_key);
            }
            cols.is_last_old = KoalaBear::ONE;
        }
        SsmcPropertyTestRow::Predecessor {
            query_key,
            anchor_key,
            next_key,
            result_val,
        } => {
            cols.query_is_predecessor = KoalaBear::ONE;
            cols.query_type = KoalaBear::new(3);
            cols.query_arg0.populate(*query_key);
            assign_value(&mut cols.result_val, result_val);
            cols.result_key.populate(*anchor_key);
            cols.uses_anchor = KoalaBear::ONE;
            cols.anchor_key.populate(*anchor_key);
            assign_value(&mut cols.anchor_val, result_val);
            cols.anchor_lt_query.populate(*anchor_key, *query_key);
            if let Some(next_key) = next_key {
                cols.next_old_key.populate(*next_key);
                populate_leq(&mut cols.query_le_next, *query_key, *next_key);
            } else {
                cols.is_last_old = KoalaBear::ONE;
            }
        }
        SsmcPropertyTestRow::EmptySuccessor { query_key } => {
            cols.query_is_successor = KoalaBear::ONE;
            cols.query_type = KoalaBear::new(2);
            cols.query_arg0.populate(*query_key);
            cols.result_is_null = KoalaBear::ONE;
            cols.uses_empty_old = KoalaBear::ONE;
        }
        SsmcPropertyTestRow::EmptyPredecessor { query_key } => {
            cols.query_is_predecessor = KoalaBear::ONE;
            cols.query_type = KoalaBear::new(3);
            cols.query_arg0.populate(*query_key);
            cols.result_is_null = KoalaBear::ONE;
            cols.uses_empty_old = KoalaBear::ONE;
        }
    }
}

/// Generate a valid SSMC property trace for the provided rows.
pub fn generate_ssmc_property_test_trace<const W: usize>(
    table_id: u32,
    col_id: u16,
    rows: &[SsmcPropertyTestRow],
) -> RowMajorMatrix<KoalaBear> {
    let width = ssmc_property_width::<W>();
    let row_count = (rows.len() + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; row_count * width];

    for (row_idx, row) in rows.iter().enumerate() {
        let cols: &mut SsmcPropertyCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[row_idx * width..(row_idx + 1) * width]);
        fill_row(cols, table_id, col_id, row);
    }

    RowMajorMatrix::new(values, width)
}

/// Canonical zero limbs for a query operand or null result key.
pub fn zero_key_limbs() -> Vec<KoalaBear> {
    u64_to_limbs(0).to_vec()
}

/// Encode one `u64` as execution/value limbs.
pub fn u64_limbs_vec(value: u64) -> Vec<KoalaBear> {
    u64_to_limbs(value).to_vec()
}

/// Canonical zero value payload for tests.
pub fn zero_value_fes<const W: usize>() -> Vec<KoalaBear> {
    zero_value::<W>()
}
