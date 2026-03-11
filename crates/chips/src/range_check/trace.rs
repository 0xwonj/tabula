//! Trace generation for the RangeCheck chip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::TraceGenerator;

use super::air::RangeCheckChip;
use super::columns::{RANGE_CHECK_SIZE, RANGE_CHECK_WIDTH, RangeCheckCols};

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
        row.multiplicity = BabyBear::ZERO;
    }

    RowMajorMatrix::new(values, RANGE_CHECK_WIDTH)
}

/// Generate a range check trace with multiplicities set from lookup counts.
///
/// `multiplicities[i]` = how many times value `i` is looked up across all chips.
pub fn generate_range_check_trace(
    multiplicities: &[u32; RANGE_CHECK_SIZE],
) -> RowMajorMatrix<BabyBear> {
    let mut values = vec![BabyBear::ZERO; RANGE_CHECK_SIZE * RANGE_CHECK_WIDTH];

    for (i, &mult) in multiplicities.iter().enumerate() {
        let offset = i * RANGE_CHECK_WIDTH;
        let row: &mut RangeCheckCols<BabyBear> =
            borrow_cols_mut(&mut values[offset..offset + RANGE_CHECK_WIDTH]);
        row.value = BabyBear::new(i as u32);
        row.multiplicity = BabyBear::new(mult);
    }

    RowMajorMatrix::new(values, RANGE_CHECK_WIDTH)
}

impl TraceGenerator for RangeCheckChip {
    type Input = [u32; RANGE_CHECK_SIZE];

    fn generate_trace(&self, input: &[u32; RANGE_CHECK_SIZE]) -> RowMajorMatrix<BabyBear> {
        generate_range_check_trace(input)
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_stark::trace::contributor::{
    TraceContributor, TracePhase, WitnessStore, witness_labels,
};
use tabula_stark::trace::trace_map::TraceMap;

impl TraceContributor for RangeCheckChip {
    fn phase(&self) -> TracePhase {
        TracePhase::DEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let mults = store.get::<Box<[u32; RANGE_CHECK_SIZE]>>(witness_labels::RANGE_CHECK_MULTS)?;
        let trace = generate_range_check_trace(mults);
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}

// ── BusConsumer impl ─────────────────────────────────────────────────────

use p3_field::PrimeField32;
use tabula_stark::air::interaction::{InteractionDirection, core_buses};
use tabula_stark::debug::RecordedInteraction;
use tabula_stark::trace::BusConsumer;

impl BusConsumer for RangeCheckChip {
    fn consumed_buses(&self) -> Vec<tabula_stark::air::BusId> {
        vec![core_buses::RANGE_CHECK]
    }

    fn collect(
        &self,
        interactions: &[RecordedInteraction<BabyBear>],
        store: &mut WitnessStore,
    ) -> Result<(), TabulaError> {
        let mut mults = [0u32; RANGE_CHECK_SIZE];
        for interaction in interactions {
            if interaction.bus != core_buses::RANGE_CHECK
                || interaction.direction != InteractionDirection::Send
            {
                continue;
            }
            if interaction.values.len() != 1 {
                return Err(TabulaError::ProofError {
                    phase: "bus_consumer",
                    detail: format!(
                        "range_check interaction width mismatch: expected 1, got {}",
                        interaction.values.len()
                    ),
                });
            }
            let mult = interaction.multiplicity.as_canonical_u32();
            if mult == 0 {
                continue;
            }
            let value = interaction.values[0].as_canonical_u32() as usize;
            if value >= RANGE_CHECK_SIZE {
                return Err(TabulaError::ProofError {
                    phase: "bus_consumer",
                    detail: format!("range_check value out of domain: {value}"),
                });
            }
            mults[value] =
                mults[value]
                    .checked_add(mult)
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "bus_consumer",
                        detail: format!("range_check multiplicity overflow at value {value}"),
                    })?;
        }
        store.put(witness_labels::RANGE_CHECK_MULTS, Box::new(mults));
        Ok(())
    }
}
