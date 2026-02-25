use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;

use crate::chips::ChipSpec;
use crate::chips::column_meta::air::ColumnMetaChip;
use crate::chips::execution::air::ExecutionChip;
use crate::chips::execution::trace::InstructionRecord;
use crate::chips::inter_tx_order::air::InterTxOrderChip;
use crate::chips::poseidon::air::PoseidonChip;
use crate::chips::range_check::generate_range_check_trace;
use crate::chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use crate::chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use crate::chips::state_column::air::StateColumnChip;
use crate::chips::static_table::air::StaticTableChip;
use crate::chips::static_table::trace::StaticTableRow;
use crate::debug::{ChipRecord, evaluate_chip, evaluate_chip_with_public_values};
use crate::trace::TraceGenerator;
use crate::witness::BatchWitness;

use super::collectors::{collect_poseidon_inputs, collect_range_check_multiplicities};
use super::memory::build_memory_traces;
use super::smt::{smt_table_public_values, validate_smt_path_shapes};
use super::trace_map::TraceMap;

/// Build all chip traces into a [`TraceMap`] from a single orchestrator entrypoint.
///
/// This function also synthesizes Poseidon and RangeCheck traces by collecting
/// C5/C8 sends from the non-preprocessed chips.
pub(super) fn build_all_traces<H, const W: usize>(
    witness: &BatchWitness<H>,
    execution_records: &[InstructionRecord],
    static_table_rows: &[StaticTableRow],
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<TraceMap, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let mut map = TraceMap::new();

    // Build memory/metadata traces (InterTxOrder, StateColumn, ColumnMeta).
    build_memory_traces::<H, W>(witness, &mut map)?;

    validate_smt_path_shapes(smt_col_paths, smt_table_paths)?;

    let exec_chip = ExecutionChip::<W>;
    let static_chip = StaticTableChip::<W>;
    let smt_col_chip = SmtColPathChip;
    let smt_table_chip = SmtTablePathChip;

    // Generate traces via TraceGenerator for validation + insertion.
    let execution_trace = exec_chip.generate_trace(execution_records);
    let smt_col_path_trace = smt_col_chip.generate_trace(smt_col_paths);
    let smt_table_path_trace = smt_table_chip.generate_trace(smt_table_paths);

    let smt_table_pvs = smt_table_public_values(witness);

    let exec_record =
        evaluate_chip(exec_chip.chip_name(), &exec_chip, &execution_trace).map_err(|e| {
            TabulaError::ProofError {
                phase: "trace_build",
                detail: format!("execution trace invalid: {e}"),
            }
        })?;
    let ito_record = evaluate_chip(
        "InterTxOrder",
        &InterTxOrderChip::<W>,
        map.get("InterTxOrder")
            .map(|e| &e.main)
            .expect("InterTxOrder trace must exist"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_build",
        detail: format!("inter_tx trace invalid: {e}"),
    })?;
    let state_record = evaluate_chip(
        "StateColumn",
        &StateColumnChip::<W>,
        map.get("StateColumn")
            .map(|e| &e.main)
            .expect("StateColumn trace must exist"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_build",
        detail: format!("state trace invalid: {e}"),
    })?;
    let col_meta_record = evaluate_chip(
        "ColumnMeta",
        &ColumnMetaChip,
        map.get("ColumnMeta")
            .map(|e| &e.main)
            .expect("ColumnMeta trace must exist"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_build",
        detail: format!("column_meta trace invalid: {e}"),
    })?;
    let smt_col_record =
        evaluate_chip(smt_col_chip.chip_name(), &smt_col_chip, &smt_col_path_trace).map_err(
            |e| TabulaError::ProofError {
                phase: "trace_build",
                detail: format!("smt_col_path trace invalid: {e}"),
            },
        )?;
    let smt_table_record = evaluate_chip_with_public_values(
        smt_table_chip.chip_name(),
        &smt_table_chip,
        &smt_table_path_trace,
        &smt_table_pvs,
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_build",
        detail: format!("smt_table_path trace invalid: {e}"),
    })?;

    let c5_c8_records: [&ChipRecord<BabyBear>; 6] = [
        &exec_record,
        &ito_record,
        &state_record,
        &col_meta_record,
        &smt_col_record,
        &smt_table_record,
    ];

    // Synthesize Poseidon and RangeCheck from collected sends.
    let poseidon_inputs = collect_poseidon_inputs(&c5_c8_records)?;
    let poseidon_chip = PoseidonChip;
    let poseidon_entry = poseidon_chip.build_entry(&poseidon_inputs);

    let range_check_mults = collect_range_check_multiplicities(&c5_c8_records)?;
    let range_check_trace = generate_range_check_trace(&range_check_mults);

    // Insert remaining traces into the map.
    map.insert(exec_chip.chip_name(), execution_trace);
    map.insert_entry(poseidon_chip.chip_name(), poseidon_entry);
    map.insert("RangeCheck", range_check_trace);
    map.insert_entry(
        static_chip.chip_name(),
        static_chip.build_entry(static_table_rows),
    );
    map.insert(smt_col_chip.chip_name(), smt_col_path_trace);
    map.insert(smt_table_chip.chip_name(), smt_table_path_trace);
    map.set_public_values(smt_table_chip.chip_name(), smt_table_pvs);

    Ok(map)
}
