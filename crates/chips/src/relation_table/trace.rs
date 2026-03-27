//! Witness trace generation for the sealed relation table lane.
#![allow(unused_imports)]

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::air::interaction::{AirInteraction, BusId};
use tabula_stark::chips::{ChipId, ChipSpec};
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use tabula_core::error::TabulaError;

use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::{TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation};
use crate::poseidon::generate_poseidon_preprocessed;

use super::air::{
    RelationTableChip, RelationTableCols, RelationTableRoundRow, relation_table_width,
};
use super::rows::{
    RELATION_TABLE_CHIP_ID, RELATION_TABLE_DOMAIN_TAG, RELATION_TABLE_WITNESS_LABEL,
    RelationTableWitnessRow,
};

impl TraceGenerator for RelationTableChip {
    type Input = [RelationTableWitnessRow];

    fn generate_trace(&self, input: &[RelationTableWitnessRow]) -> RowMajorMatrix<KoalaBear> {
        build_trace_and_root(input).0
    }
}

impl TraceContributor for RelationTableChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let rows = store.get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)?;
        let (trace, root) = build_trace_and_root(rows.as_slice());
        map.insert_with_preprocessed(
            self.chip_id(),
            trace,
            generate_poseidon_preprocessed(1 + rows.len() * 3),
        );
        map.set_public_values(self.chip_id(), root.to_vec());
        Ok(())
    }
}

fn build_round_rows(rows: &[RelationTableWitnessRow]) -> Vec<RelationTableRoundRow> {
    let row_count = rows.len() as u32;
    let mut round_rows = Vec::new();
    let mut prev_digest = [0u32; 8];

    let header_block = {
        let mut block = [KoalaBear::ZERO; 8];
        block[0] = KoalaBear::new(RELATION_TABLE_DOMAIN_TAG);
        block[1] = KoalaBear::new(row_count);
        block
    };
    let header_terminal = rows.is_empty();
    append_block_rows(
        &mut round_rows,
        row_count,
        0,
        [0; 8],
        [0; 8],
        0,
        prev_digest,
        header_block,
        true,
        false,
        false,
        false,
        header_terminal,
    );
    prev_digest = digest_words(&round_rows.last().expect("header rows").perm_state_out);

    for (index, row) in rows.iter().enumerate() {
        let is_last_row = index + 1 == rows.len();

        let mut first = [KoalaBear::ZERO; 8];
        first[0] = KoalaBear::new(row.relation_id);
        for (idx, value) in row.input_digest.iter().take(7).enumerate() {
            first[1 + idx] = KoalaBear::new(*value);
        }
        append_block_rows(
            &mut round_rows,
            row_count,
            row.relation_id,
            row.input_digest,
            row.output_digest,
            row.lookup_mult,
            prev_digest,
            first,
            false,
            true,
            false,
            false,
            false,
        );
        prev_digest = digest_words(&round_rows.last().expect("row0 rows").perm_state_out);

        let mut second = [KoalaBear::ZERO; 8];
        second[0] = KoalaBear::new(row.input_digest[7]);
        for (idx, value) in row.output_digest.iter().take(7).enumerate() {
            second[1 + idx] = KoalaBear::new(*value);
        }
        append_block_rows(
            &mut round_rows,
            row_count,
            row.relation_id,
            row.input_digest,
            row.output_digest,
            row.lookup_mult,
            prev_digest,
            second,
            false,
            false,
            true,
            false,
            false,
        );
        prev_digest = digest_words(&round_rows.last().expect("row1 rows").perm_state_out);

        let mut third = [KoalaBear::ZERO; 8];
        third[0] = KoalaBear::new(row.output_digest[7]);
        append_block_rows(
            &mut round_rows,
            row_count,
            row.relation_id,
            row.input_digest,
            row.output_digest,
            row.lookup_mult,
            prev_digest,
            third,
            false,
            false,
            false,
            true,
            is_last_row,
        );
        prev_digest = digest_words(&round_rows.last().expect("row2 rows").perm_state_out);
    }

    round_rows
}

#[allow(clippy::too_many_arguments)]
fn append_block_rows(
    rows: &mut Vec<RelationTableRoundRow>,
    row_count: u32,
    relation_id: u32,
    input_digest: [u32; 8],
    output_digest: [u32; 8],
    lookup_mult: u32,
    prev_digest: [u32; 8],
    block_values: [KoalaBear; 8],
    phase_header: bool,
    phase_row0: bool,
    phase_row1: bool,
    phase_row2: bool,
    is_terminal_block: bool,
) {
    let mut perm_input = [KoalaBear::ZERO; WIDTH];
    for (idx, digest_word) in prev_digest.iter().enumerate() {
        perm_input[idx] = KoalaBear::new(*digest_word);
        perm_input[8 + idx] = block_values[idx];
    }
    let (rounds, output_state) = poseidon2_permutation(perm_input);
    for (round_ctr, round_data) in rounds.into_iter().enumerate() {
        rows.push(RelationTableRoundRow {
            is_terminal_block,
            phase_header,
            phase_row0,
            phase_row1,
            phase_row2,
            row_count,
            relation_id,
            input_digest,
            output_digest,
            lookup_mult,
            prev_digest,
            block_values,
            perm_state_out: output_state,
            round_ctr: round_ctr as u32,
            round_data,
            perm_input,
            perm_output: core::array::from_fn(|idx| output_state[idx]),
        });
    }
}

fn build_trace_and_root(
    rows: &[RelationTableWitnessRow],
) -> (RowMajorMatrix<KoalaBear>, [KoalaBear; 8]) {
    let width = relation_table_width();
    let round_rows = build_round_rows(rows);
    let num_real = round_rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];

    for (row_index, row) in round_rows.iter().enumerate() {
        let offset = row_index * width;
        let cols: &mut RelationTableCols<KoalaBear> =
            borrow_cols_mut(&mut values[offset..offset + width]);
        cols.is_real = KoalaBear::ONE;
        cols.is_terminal_block = if row.is_terminal_block {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.phase_header = if row.phase_header {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.phase_row0 = if row.phase_row0 {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.phase_row1 = if row.phase_row1 {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.phase_row2 = if row.phase_row2 {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.row_count = KoalaBear::new(row.row_count);
        cols.relation_id = KoalaBear::new(row.relation_id);
        for idx in 0..8 {
            cols.input_digest[idx] = KoalaBear::new(row.input_digest[idx]);
            cols.output_digest[idx] = KoalaBear::new(row.output_digest[idx]);
            cols.prev_digest[idx] = KoalaBear::new(row.prev_digest[idx]);
        }
        cols.lookup_mult = KoalaBear::new(row.lookup_mult);
        cols.block_values = row.block_values;
        cols.perm_state_out = row.perm_state_out;
        cols.poseidon.is_real = KoalaBear::ONE;
        cols.poseidon.is_first_round = if row.round_ctr == 0 {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.poseidon.is_last_round = if row.round_ctr + 1 == TOTAL_ROUNDS as u32 {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.poseidon.is_full_round = if is_full_round(row.round_ctr as usize) {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        };
        cols.poseidon.round_ctr = KoalaBear::new(row.round_ctr);
        cols.poseidon.perm_input = row.perm_input;
        cols.poseidon.rc = row.round_data.rc;
        cols.poseidon.state = row.round_data.state_before;
        cols.poseidon.sbox_y2 = row.round_data.sbox_y2;
        cols.poseidon.sbox_y3 = row.round_data.sbox_y3;
        cols.poseidon.perm_output = row.perm_output;
    }

    let root = round_rows.last().map_or([KoalaBear::ZERO; 8], |row| {
        core::array::from_fn(|idx| row.perm_state_out[idx])
    });
    (RowMajorMatrix::new(values, width), root)
}

fn digest_words(state: &[KoalaBear; WIDTH]) -> [u32; 8] {
    core::array::from_fn(|idx| state[idx].as_canonical_u32())
}

#[cfg(test)]
mod tests {
    use super::*;

    use tabula_stark::debug::debug_check_with_public_values;

    fn row() -> RelationTableWitnessRow {
        RelationTableWitnessRow {
            relation_id: 1,
            input_digest: core::array::from_fn(|idx| idx as u32 + 10),
            output_digest: core::array::from_fn(|idx| idx as u32 + 20),
            lookup_mult: 1,
        }
    }

    #[test]
    fn relation_table_real_flag_mismatch_fails() {
        let chip = RelationTableChip;
        let witness = vec![row()];
        let mut trace = chip.generate_trace(&witness);
        let pvs = build_trace_and_root(&witness).1.to_vec();
        let width = relation_table_width();
        let row = &mut trace.values[0..width];
        let cols: &mut RelationTableCols<KoalaBear> = borrow_cols_mut(row);
        cols.poseidon.is_real = KoalaBear::ZERO;

        debug_check_with_public_values(&chip, &trace, &pvs)
            .expect_err("split real flags must fail");
    }

    #[test]
    fn relation_table_must_end_on_terminal_block() {
        let chip = RelationTableChip;
        let witness = vec![row()];
        let mut trace = chip.generate_trace(&witness);
        let pvs = build_trace_and_root(&witness).1.to_vec();
        let last_real_row = 4 * TOTAL_ROUNDS - 1;
        let width = relation_table_width();
        let row = &mut trace.values[last_real_row * width..(last_real_row + 1) * width];
        let cols: &mut RelationTableCols<KoalaBear> = borrow_cols_mut(row);
        cols.is_terminal_block = KoalaBear::ZERO;

        debug_check_with_public_values(&chip, &trace, &pvs)
            .expect_err("ending mid-table must fail");
    }
}
