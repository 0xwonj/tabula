use tabula_commitment::NativeDigest;
use tabula_core::error::TabulaError;

use crate::air::chips::column_meta::air::ColumnMetaChip;
use crate::air::chips::execution::air::ExecutionChip;
use crate::air::chips::inter_tx_order::air::InterTxOrderChip;
use crate::air::chips::poseidon::air::PoseidonChip;
use crate::air::chips::range_check::RangeCheckChip;
use crate::air::chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use crate::air::chips::state_column::air::StateColumnChip;
use crate::air::chips::static_table::air::StaticTableChip;
use crate::air::debug::{
    check_bus_balance, debug_check, debug_check_with_preprocessed, debug_check_with_public_values,
    evaluate_chip, evaluate_chip_with_preprocessed, evaluate_chip_with_public_values,
};
use crate::air::interaction::InteractionKind;

use super::smt::smt_table_public_values_from_roots;
use super::types::AllTraceBundle;

/// Validate an all-chip bundle with debug constraints and bus balance checks.
pub(super) fn debug_validate_all_trace_bundle<const W: usize>(
    bundle: &AllTraceBundle<W>,
    old_state_root: &NativeDigest,
    new_state_root: &NativeDigest,
) -> Result<(), TabulaError> {
    debug_check(&ExecutionChip::<W>, &bundle.execution_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("execution debug_check failed: {e}")))?;
    debug_check(&InterTxOrderChip::<W>, &bundle.memory.inter_tx_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("inter_tx_order debug_check failed: {e}"))
    })?;
    debug_check(&StateColumnChip::<W>, &bundle.memory.state_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("state_column debug_check failed: {e}"))
    })?;
    debug_check(&ColumnMetaChip, &bundle.memory.column_meta_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("column_meta debug_check failed: {e}"))
    })?;
    debug_check(&StaticTableChip::<W>, &bundle.static_table_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("static_table debug_check failed: {e}"))
    })?;
    debug_check(&SmtColPathChip, &bundle.smt_col_path_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("smt_col_path debug_check failed: {e}"))
    })?;

    let smt_table_pvs = smt_table_public_values_from_roots(old_state_root, new_state_root);
    debug_check_with_public_values(
        &SmtTablePathChip,
        &bundle.smt_table_path_trace,
        &smt_table_pvs,
    )
    .map_err(|e| {
        TabulaError::ConsistencyError(format!("smt_table_path debug_check failed: {e}"))
    })?;
    debug_check_with_preprocessed(
        &PoseidonChip,
        &bundle.poseidon_trace,
        Some(&bundle.poseidon_preprocessed_trace),
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("poseidon debug_check failed: {e}")))?;
    debug_check(&RangeCheckChip, &bundle.range_check_trace).map_err(|e| {
        TabulaError::ConsistencyError(format!("range_check debug_check failed: {e}"))
    })?;

    let execution = evaluate_chip("Execution", &ExecutionChip::<W>, &bundle.execution_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("execution evaluate failed: {e}")))?;
    let inter_tx = evaluate_chip(
        "InterTxOrder",
        &InterTxOrderChip::<W>,
        &bundle.memory.inter_tx_trace,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("inter_tx evaluate failed: {e}")))?;
    let state = evaluate_chip(
        "StateColumn",
        &StateColumnChip::<W>,
        &bundle.memory.state_trace,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("state evaluate failed: {e}")))?;
    let col_meta = evaluate_chip(
        "ColumnMeta",
        &ColumnMetaChip,
        &bundle.memory.column_meta_trace,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("column_meta evaluate failed: {e}")))?;
    let static_table = evaluate_chip(
        "StaticTable",
        &StaticTableChip::<W>,
        &bundle.static_table_trace,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("static_table evaluate failed: {e}")))?;
    let smt_col = evaluate_chip("SmtColPath", &SmtColPathChip, &bundle.smt_col_path_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("smt_col evaluate failed: {e}")))?;
    let smt_table = evaluate_chip_with_public_values(
        "SmtTablePath",
        &SmtTablePathChip,
        &bundle.smt_table_path_trace,
        &smt_table_pvs,
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("smt_table evaluate failed: {e}")))?;
    let poseidon = evaluate_chip_with_preprocessed(
        "Poseidon",
        &PoseidonChip,
        &bundle.poseidon_trace,
        Some(&bundle.poseidon_preprocessed_trace),
    )
    .map_err(|e| TabulaError::ConsistencyError(format!("poseidon evaluate failed: {e}")))?;
    let range_check = evaluate_chip("RangeCheck", &RangeCheckChip, &bundle.range_check_trace)
        .map_err(|e| TabulaError::ConsistencyError(format!("range_check evaluate failed: {e}")))?;

    let records = [
        execution,
        inter_tx,
        state,
        col_meta,
        static_table,
        smt_col,
        smt_table,
        poseidon,
        range_check,
    ];

    for bus in [
        InteractionKind::PoseidonPermutation,
        InteractionKind::CommitmentVerification,
        InteractionKind::RangeCheck,
        InteractionKind::StaticTableLookup,
        InteractionKind::ReadAccess,
        InteractionKind::WriteAccess,
        InteractionKind::EmptyColRead,
        InteractionKind::BaseStateEntry,
        InteractionKind::CoalescedWrite,
        InteractionKind::SmtLeafDigest,
        InteractionKind::SmtTableRoot,
    ] {
        check_bus_balance(&records, bus)
            .map_err(|e| TabulaError::ConsistencyError(format!("bus {bus:?} imbalance: {e}")))?;
    }

    Ok(())
}
