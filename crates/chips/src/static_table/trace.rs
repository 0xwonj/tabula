//! Trace generation for the StaticTable chip.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::primitives::encode_u64_limbs;

use tabula_stark::air::columns::borrow_cols_mut;

use super::columns::{StaticTableCols, static_table_width};

/// A single static table entry for trace generation.
#[derive(Debug, Clone)]
pub struct StaticTableRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key.
    pub row_key: u64,
    /// Value (field elements, length must match W).
    pub value: Vec<KoalaBear>,
    /// Multiplicity of this row on C9 StaticTableLookup bus.
    ///
    /// Set to the number of matching `Lookup` sends in the Execution trace.
    pub lookup_mult: u32,
}

/// Generate a StaticTable trace from witness rows.
///
/// Rows are padded to the next power of two. Padding rows have `is_real = 0`.
pub fn generate_static_table_trace<const W: usize>(
    rows: &[StaticTableRow],
) -> RowMajorMatrix<KoalaBear> {
    let width = static_table_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row.value.len(),
            W,
            "static table value width mismatch: got {}, expected {W}",
            row.value.len()
        );

        let offset = i * width;
        let slice = &mut values[offset..offset + width];
        let cols: &mut StaticTableCols<KoalaBear, W> = borrow_cols_mut(slice);

        cols.is_real = KoalaBear::ONE;
        cols.table_id = KoalaBear::new(row.table_id);
        cols.col_id = KoalaBear::new(row.col_id as u32);

        let limbs = encode_u64_limbs(row.row_key);
        cols.row_key.limb0 = limbs[0];
        cols.row_key.limb1 = limbs[1];
        cols.row_key.limb2 = limbs[2];

        cols.value.copy_from_slice(&row.value);
        cols.lookup_mult_witness = KoalaBear::new(row.lookup_mult);
    }

    RowMajorMatrix::new(values, width)
}

// ── TraceGenerator impl ─────────────────────────────────────────────────────

use tabula_stark::trace::TraceGenerator;

impl<const W: usize> TraceGenerator for super::air::StaticTableChip<W> {
    type Input = [StaticTableRow];

    fn generate_trace(&self, input: &[StaticTableRow]) -> RowMajorMatrix<KoalaBear> {
        generate_static_table_trace::<W>(input)
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_stark::trace::contributor::{
    TraceContributor, TracePhase, WitnessStore, witness_labels,
};
use tabula_stark::trace::trace_map::TraceMap;

impl<const W: usize> TraceContributor for super::air::StaticTableChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let rows = store.get::<Vec<StaticTableRow>>(witness_labels::STATIC_TABLE_ROWS)?;
        let entry = self.build_entry(rows);
        map.insert_entry(self.chip_id(), entry);
        Ok(())
    }
}
