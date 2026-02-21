use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ExecutionResult, TableId, TableSchema};

use crate::air::chips::column_meta::air::ColumnMetaChip;
use crate::air::chips::execution::air::ExecutionChip;
use crate::air::chips::execution::trace::{InstructionRecord, generate_execution_trace};
use crate::air::chips::inter_tx_order::air::InterTxOrderChip;
use crate::air::chips::poseidon::trace::{generate_poseidon_preprocessed, generate_poseidon_trace};
use crate::air::chips::range_check::generate_range_check_trace;
use crate::air::chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use crate::air::chips::smt_path::trace::{
    SmtPathWitness, SmtTablePathWitness, generate_smt_col_path_trace, generate_smt_table_path_trace,
};
use crate::air::chips::state_column::air::StateColumnChip;
use crate::air::chips::static_table::trace::{StaticTableRow, generate_static_table_trace};
use crate::air::debug::{ChipRecord, evaluate_chip, evaluate_chip_with_public_values};
use crate::witness::BatchWitness;

use super::collectors::{collect_poseidon_inputs, collect_range_check_multiplicities};
use super::lowering::lower_execution_records;
use super::memory::build_trace_bundle;
use super::smt::{smt_table_public_values, validate_smt_path_shapes};
use super::types::AllTraceBundle;

/// Build all-chip traces from a single orchestrator entrypoint.
///
/// This function also synthesizes Poseidon and RangeCheck traces by collecting
/// C5/C8 sends from the non-preprocessed chips.
pub(super) fn build_all_trace_bundle<H, const W: usize>(
    witness: &BatchWitness<H>,
    execution_records: &[InstructionRecord],
    static_table_rows: &[StaticTableRow],
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<AllTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let memory = build_trace_bundle::<H, W>(witness)?;
    validate_smt_path_shapes(smt_col_paths, smt_table_paths)?;

    let execution_trace = generate_execution_trace::<W>(execution_records);
    let static_table_trace = generate_static_table_trace::<W>(static_table_rows);
    let smt_col_path_trace = generate_smt_col_path_trace(smt_col_paths);
    let smt_table_path_trace = generate_smt_table_path_trace(smt_table_paths);

    let smt_table_pvs = smt_table_public_values(witness);

    let exec_record = evaluate_chip("Execution", &ExecutionChip::<W>, &execution_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("execution trace invalid: {e}")))?;
    let ito_record = evaluate_chip(
        "InterTxOrder",
        &InterTxOrderChip::<W>,
        &memory.inter_tx_trace,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("inter_tx trace invalid: {e}")))?;
    let state_record = evaluate_chip("StateColumn", &StateColumnChip::<W>, &memory.state_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("state trace invalid: {e}")))?;
    let col_meta_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &memory.column_meta_trace)
        .map_err(|e| {
        TabulaError::ConsistencyError(format!("column_meta trace invalid: {e}"))
    })?;
    let smt_col_record = evaluate_chip("SmtColPath", &SmtColPathChip, &smt_col_path_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("smt_col_path trace invalid: {e}")))?;
    let smt_table_record = evaluate_chip_with_public_values(
        "SmtTablePath",
        &SmtTablePathChip,
        &smt_table_path_trace,
        &smt_table_pvs,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("smt_table_path trace invalid: {e}")))?;

    let c5_c8_records: [&ChipRecord<BabyBear>; 6] = [
        &exec_record,
        &ito_record,
        &state_record,
        &col_meta_record,
        &smt_col_record,
        &smt_table_record,
    ];

    let poseidon_inputs = collect_poseidon_inputs(&c5_c8_records)?;
    let poseidon_trace = generate_poseidon_trace(&poseidon_inputs);
    let poseidon_preprocessed_trace = generate_poseidon_preprocessed(poseidon_inputs.len());

    let range_check_mults = collect_range_check_multiplicities(&c5_c8_records)?;
    let range_check_trace = generate_range_check_trace(&range_check_mults);

    Ok(AllTraceBundle {
        memory,
        execution_trace,
        static_table_trace,
        smt_col_path_trace,
        smt_table_path_trace,
        poseidon_trace,
        poseidon_preprocessed_trace,
        range_check_trace,
    })
}

/// Build all-chip traces directly from `ExecutionResult` via access-event lowering.
pub(super) fn build_all_trace_bundle_from_execution_result<H, const W: usize>(
    witness: &BatchWitness<H>,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_table_rows: &[StaticTableRow],
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<AllTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let execution_records = lower_execution_records::<W>(execution_result, schemas)?;
    build_all_trace_bundle::<H, W>(
        witness,
        &execution_records,
        static_table_rows,
        smt_col_paths,
        smt_table_paths,
    )
}
