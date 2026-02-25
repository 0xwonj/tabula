use tabula_commitment::NativeDigest;
use tabula_core::error::TabulaError;

use crate::air::interaction::InteractionKind;
use crate::chips::column_meta::air::ColumnMetaChip;
use crate::chips::execution::air::ExecutionChip;
use crate::chips::inter_tx_order::air::InterTxOrderChip;
use crate::chips::poseidon::air::PoseidonChip;
use crate::chips::range_check::RangeCheckChip;
use crate::chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use crate::chips::state_column::air::StateColumnChip;
use crate::chips::static_table::air::StaticTableChip;
use crate::debug::{
    check_bus_balance, debug_check, debug_check_with_preprocessed, debug_check_with_public_values,
    evaluate_chip, evaluate_chip_with_preprocessed, evaluate_chip_with_public_values,
};

use super::smt::smt_table_public_values_from_roots;
use super::trace_map::TraceMap;

/// Validate all chip traces in a [`TraceMap`] with debug constraints and bus balance checks.
pub(super) fn debug_validate_trace_map<const W: usize>(
    map: &TraceMap,
    old_state_root: &NativeDigest,
    new_state_root: &NativeDigest,
) -> Result<(), TabulaError> {
    let get_main = |name: &str| {
        map.get(name)
            .map(|e| &e.main)
            .expect("chip trace must exist")
    };

    // ── Debug constraint checks ───────────────────────────────────────────
    debug_check(&ExecutionChip::<W>, get_main("Execution")).map_err(|e| {
        TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("execution debug_check failed: {e}"),
        }
    })?;
    debug_check(&InterTxOrderChip::<W>, get_main("InterTxOrder")).map_err(|e| {
        TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("inter_tx_order debug_check failed: {e}"),
        }
    })?;
    debug_check(&StateColumnChip::<W>, get_main("StateColumn")).map_err(|e| {
        TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("state_column debug_check failed: {e}"),
        }
    })?;
    debug_check(&ColumnMetaChip, get_main("ColumnMeta")).map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("column_meta debug_check failed: {e}"),
    })?;
    debug_check(&StaticTableChip::<W>, get_main("StaticTable")).map_err(|e| {
        TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("static_table debug_check failed: {e}"),
        }
    })?;
    debug_check(&SmtColPathChip, get_main("SmtColPath")).map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("smt_col_path debug_check failed: {e}"),
    })?;

    let smt_table_pvs = smt_table_public_values_from_roots(old_state_root, new_state_root);
    debug_check_with_public_values(&SmtTablePathChip, get_main("SmtTablePath"), &smt_table_pvs)
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("smt_table_path debug_check failed: {e}"),
        })?;

    let poseidon_entry = map.get("Poseidon").expect("Poseidon trace must exist");
    debug_check_with_preprocessed(
        &PoseidonChip,
        &poseidon_entry.main,
        poseidon_entry.preprocessed.as_ref(),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("poseidon debug_check failed: {e}"),
    })?;
    debug_check(&RangeCheckChip, get_main("RangeCheck")).map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("range_check debug_check failed: {e}"),
    })?;

    // ── Bus balance checks ────────────────────────────────────────────────
    let execution = evaluate_chip("Execution", &ExecutionChip::<W>, get_main("Execution"))
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("execution evaluate failed: {e}"),
        })?;
    let inter_tx = evaluate_chip(
        "InterTxOrder",
        &InterTxOrderChip::<W>,
        get_main("InterTxOrder"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("inter_tx evaluate failed: {e}"),
    })?;
    let state = evaluate_chip(
        "StateColumn",
        &StateColumnChip::<W>,
        get_main("StateColumn"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("state evaluate failed: {e}"),
    })?;
    let col_meta =
        evaluate_chip("ColumnMeta", &ColumnMetaChip, get_main("ColumnMeta")).map_err(|e| {
            TabulaError::ProofError {
                phase: "trace_validation",
                detail: format!("column_meta evaluate failed: {e}"),
            }
        })?;
    let static_table = evaluate_chip(
        "StaticTable",
        &StaticTableChip::<W>,
        get_main("StaticTable"),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("static_table evaluate failed: {e}"),
    })?;
    let smt_col =
        evaluate_chip("SmtColPath", &SmtColPathChip, get_main("SmtColPath")).map_err(|e| {
            TabulaError::ProofError {
                phase: "trace_validation",
                detail: format!("smt_col evaluate failed: {e}"),
            }
        })?;
    let smt_table = evaluate_chip_with_public_values(
        "SmtTablePath",
        &SmtTablePathChip,
        get_main("SmtTablePath"),
        &smt_table_pvs,
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("smt_table evaluate failed: {e}"),
    })?;
    let poseidon = evaluate_chip_with_preprocessed(
        "Poseidon",
        &PoseidonChip,
        &poseidon_entry.main,
        poseidon_entry.preprocessed.as_ref(),
    )
    .map_err(|e| TabulaError::ProofError {
        phase: "trace_validation",
        detail: format!("poseidon evaluate failed: {e}"),
    })?;
    let range_check = evaluate_chip("RangeCheck", &RangeCheckChip, get_main("RangeCheck"))
        .map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("range_check evaluate failed: {e}"),
        })?;

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
        check_bus_balance(&records, bus).map_err(|e| TabulaError::ProofError {
            phase: "trace_validation",
            detail: format!("bus {bus:?} imbalance: {e}"),
        })?;
    }

    Ok(())
}
