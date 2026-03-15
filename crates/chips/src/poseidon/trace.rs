//! Trace generation for the PoseidonChip.
//!
//! Converts a list of permutation inputs into a `RowMajorMatrix<KoalaBear>` trace
//! with 21 rows per permutation (one per round).

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;

use super::columns::{
    POSEIDON_PREPROCESSED_WIDTH, PoseidonCols, PoseidonPreprocessedCols, poseidon_width,
};
use super::constants::{
    TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation, round_constants,
};

/// Generate a Poseidon2 trace from a list of permutation inputs.
///
/// Each input is a 16-element KoalaBear vector. The trace has 21 rows per
/// permutation, padded to a power of 2.
pub fn generate_poseidon_trace(inputs: &[[KoalaBear; WIDTH]]) -> RowMajorMatrix<KoalaBear> {
    let width = poseidon_width();
    let num_real = inputs.len() * TOTAL_ROUNDS;
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];

    for (perm_idx, input) in inputs.iter().enumerate() {
        let (rounds, output) = poseidon2_permutation(*input);
        debug_assert_eq!(rounds.len(), TOTAL_ROUNDS);

        // perm_output = first 8 elements of the permutation output (digest).
        let perm_output: [KoalaBear; 8] = core::array::from_fn(|j| output[j]);

        for (r, round_data) in rounds.iter().enumerate() {
            let row_idx = perm_idx * TOTAL_ROUNDS + r;
            let offset = row_idx * width;
            let cols: &mut PoseidonCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + width]);

            cols.is_real = KoalaBear::ONE;
            cols.state = round_data.state_before;
            cols.rc = round_data.rc;
            cols.sbox_y2 = round_data.sbox_y2;
            cols.sbox_y3 = round_data.sbox_y3;
            cols.round_ctr = KoalaBear::new(r as u32);
            cols.is_full_round = bool_fe(is_full_round(r));
            cols.is_first_round = bool_fe(r == 0);
            cols.is_last_round = bool_fe(r == TOTAL_ROUNDS - 1);
            cols.perm_input = *input;
            cols.perm_output = perm_output;
        }
    }

    RowMajorMatrix::new(values, width)
}

/// Generate the preprocessed trace for PoseidonChip.
///
/// Contains the expected round constants and `is_full_round` flag for each row.
/// Cycles through the 21-round pattern for each permutation, with zero padding.
/// Must be the same height as the main trace.
pub fn generate_poseidon_preprocessed(num_perms: usize) -> RowMajorMatrix<KoalaBear> {
    let num_real = num_perms * TOTAL_ROUNDS;
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * POSEIDON_PREPROCESSED_WIDTH];

    for perm_idx in 0..num_perms {
        for r in 0..TOTAL_ROUNDS {
            let row_idx = perm_idx * TOTAL_ROUNDS + r;
            let offset = row_idx * POSEIDON_PREPROCESSED_WIDTH;
            let cols: &mut PoseidonPreprocessedCols<KoalaBear> =
                borrow_cols_mut(&mut values[offset..offset + POSEIDON_PREPROCESSED_WIDTH]);

            cols.rc = round_constants(r);
            cols.is_full_round = bool_fe(is_full_round(r));
            cols.is_first_round = bool_fe(r == 0);
            cols.is_last_round = bool_fe(r == TOTAL_ROUNDS - 1);
        }
    }
    // Padding rows remain zero (rc=0, all flags=0).

    RowMajorMatrix::new(values, POSEIDON_PREPROCESSED_WIDTH)
}

// ── TraceGenerator impl ─────────────────────────────────────────────────────

use tabula_stark::trace::TraceGenerator;

impl TraceGenerator for super::air::PoseidonChip {
    type Input = [[KoalaBear; WIDTH]];

    fn generate_trace(&self, input: &[[KoalaBear; WIDTH]]) -> RowMajorMatrix<KoalaBear> {
        generate_poseidon_trace(input)
    }

    fn generate_preprocessed(
        &self,
        input: &[[KoalaBear; WIDTH]],
    ) -> Option<RowMajorMatrix<KoalaBear>> {
        Some(generate_poseidon_preprocessed(input.len()))
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_stark::trace::contributor::{
    TraceContributor, TracePhase, WitnessStore, witness_labels,
};
use tabula_stark::trace::trace_map::TraceMap;

impl TraceContributor for super::air::PoseidonChip {
    fn phase(&self) -> TracePhase {
        TracePhase::DEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let inputs = store.get::<Vec<[KoalaBear; WIDTH]>>(witness_labels::POSEIDON_INPUTS)?;
        let entry = self.build_entry(inputs);
        map.insert_entry(self.chip_id(), entry);
        Ok(())
    }
}

// ── BusConsumer impl ─────────────────────────────────────────────────────

use p3_field::PrimeField32;
use tabula_stark::air::interaction::{InteractionDirection, core_buses};
use tabula_stark::debug::RecordedInteraction;
use tabula_stark::trace::BusConsumer;

impl BusConsumer for super::air::PoseidonChip {
    fn consumed_buses(&self) -> Vec<tabula_stark::air::BusId> {
        vec![core_buses::POSEIDON_PERM]
    }

    fn collect(
        &self,
        interactions: &[RecordedInteraction<KoalaBear>],
        store: &mut WitnessStore,
    ) -> Result<(), TabulaError> {
        let mut inputs = Vec::new();
        for interaction in interactions {
            if interaction.bus != core_buses::POSEIDON_PERM
                || interaction.direction != InteractionDirection::Send
            {
                continue;
            }
            if interaction.values.len() != 24 {
                return Err(TabulaError::ProofError {
                    phase: "bus_consumer",
                    detail: format!(
                        "poseidon interaction width mismatch: expected 24, got {}",
                        interaction.values.len()
                    ),
                });
            }
            let mult = interaction.multiplicity.as_canonical_u32();
            if mult == 0 {
                continue;
            }
            let mut input = [KoalaBear::ZERO; 16];
            input.copy_from_slice(&interaction.values[..16]);
            for _ in 0..mult {
                inputs.push(input);
            }
        }
        store.put(witness_labels::POSEIDON_INPUTS, inputs);
        Ok(())
    }
}
