//! Trace generation for the PropertyVerifier chip.
//!
//! Converts per-column property read records into a `RowMajorMatrix<KoalaBear>`
//! trace. Each real row corresponds to one PropertyRead query targeting this column.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_core::error::TabulaError;
use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::generator::TraceGenerator;
use tabula_stark::trace::trace_map::TraceMap;

use crate::ChipSpec;

use super::air::PropertyVerifierChip;
use super::columns::{PropertyVerifierCols, property_verifier_width};

/// WitnessStore label for property read records in a column store.
pub const PROPERTY_READ_WITNESS_LABEL: &str = "property_read_records";

/// Witness record for a single PropertyRead query result.
///
/// Populated during witness lowering and stored per-column in the
/// column tier's WitnessStore.
#[derive(Debug, Clone)]
pub struct PropertyReadRecord {
    /// Query type ordinal (0=Minimum, 1=Maximum, etc.).
    pub query_type: u8,
    /// Result value as field elements (length W).
    pub result_val: Vec<KoalaBear>,
    /// Result key as field elements (length W).
    pub result_key: Vec<KoalaBear>,
    /// Whether the result is null.
    pub is_null: bool,
}

/// Generate the PropertyVerifier trace from witness records.
pub fn generate_property_verifier_trace<const W: usize>(
    table_id: u32,
    col_id: u16,
    records: &[PropertyReadRecord],
) -> RowMajorMatrix<KoalaBear> {
    let width = property_verifier_width::<W>();
    let n_real = records.len();
    let n_rows = (n_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; n_rows * width];

    for (row_idx, rec) in records.iter().enumerate() {
        let row = &mut values[row_idx * width..(row_idx + 1) * width];
        let cols: &mut PropertyVerifierCols<KoalaBear, W> = borrow_cols_mut(row);

        cols.is_real = KoalaBear::ONE;
        cols.table_id = KoalaBear::new(table_id);
        cols.col_id = KoalaBear::new(col_id as u32);
        cols.query_type = KoalaBear::new(rec.query_type as u32);
        for i in 0..W {
            cols.result_val[i] = rec.result_val.get(i).copied().unwrap_or(KoalaBear::ZERO);
            cols.result_key[i] = rec.result_key.get(i).copied().unwrap_or(KoalaBear::ZERO);
        }
        cols.is_null = bool_fe(rec.is_null);
    }

    RowMajorMatrix::new(values, width)
}

// -- TraceGenerator impl -------------------------------------------------

/// Input bundle for `PropertyVerifierChip` trace generation.
pub struct PropertyVerifierInput {
    /// Witness records for this column's property queries.
    pub records: Vec<PropertyReadRecord>,
}

impl<const W: usize> TraceGenerator for PropertyVerifierChip<W> {
    type Input = PropertyVerifierInput;

    fn generate_trace(&self, input: &PropertyVerifierInput) -> RowMajorMatrix<KoalaBear> {
        generate_property_verifier_trace::<W>(self.table_id(), self.col_id(), &input.records)
    }
}

// -- TraceContributor impl -----------------------------------------------

impl<const W: usize> TraceContributor for PropertyVerifierChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let empty = Vec::new();
        let records = store
            .get::<Vec<PropertyReadRecord>>(PROPERTY_READ_WITNESS_LABEL)
            .unwrap_or(&empty);

        let trace = generate_property_verifier_trace::<W>(self.table_id(), self.col_id(), records);
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
