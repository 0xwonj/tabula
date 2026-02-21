//! Handler for the `execute` subcommand.

use std::collections::BTreeMap;

use tabula_artifact::{BatchFile, StateFile, merge_output_state_cells, normalize_state};
use tabula_core::Batch;
use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;

use crate::io::{ExecutionOutput, StateCell, load_json, write_json};

pub fn cmd_execute(
    program_path: &std::path::Path,
    state_path: &std::path::Path,
    batch_path: &std::path::Path,
    output_state_path: Option<&std::path::Path>,
    include_trace: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    // 1. Load + register program
    let registered = tabula_driver::load_and_register_program(program_path)?;

    // 2. Load state + batch
    let state_file: StateFile = load_json(state_path)?;
    let batch_file: BatchFile = load_json(batch_path)?;
    let normalized =
        normalize_state(&state_file).map_err(|e| anyhow::anyhow!("invalid state cell: {e}"))?;

    // 3. Build InMemoryState
    let mut state_store = InMemoryState::new();
    for cell in &normalized.cells {
        let (key, value) = cell
            .to_cell_pair()
            .map_err(|e| anyhow::anyhow!("invalid state cell: {e}"))?;
        state_store.set(key, value);
    }

    // 4. Convert transactions + execute
    let transactions: Vec<_> = batch_file
        .transactions
        .iter()
        .map(|t| {
            t.to_transaction()
                .map_err(|e| anyhow::anyhow!("invalid batch tx: {e}"))
        })
        .collect::<Result<_, _>>()?;
    let batch = Batch { transactions };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &MockHasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
    };

    let result = execute_batch(
        &batch,
        &registered.program,
        &state_store,
        &env,
        &BTreeMap::new(),
    )?;

    // 5. Post-process
    let consistency = check_consistency_status(&result.events, &result.read_set_old);
    let state_after = StateFile {
        cells: merge_output_state_cells(&normalized.cells, &result.write_set_final),
    };

    if let Some(out_path) = output_state_path {
        write_json(out_path, &state_after)?;
    }

    let read_set: Vec<StateCell> = result
        .read_set_old
        .iter()
        .map(|(k, v)| StateCell::from_cell_pair(k, v))
        .collect();
    let write_set: Vec<StateCell> = result
        .write_set_final
        .iter()
        .map(|(k, v)| StateCell::from_cell_pair(k, v))
        .collect();
    let trace = if include_trace {
        Some(result.events.clone())
    } else {
        None
    };

    if json_output {
        let output = ExecutionOutput {
            tx_outcomes: result.tx_outcomes.clone(),
            read_set,
            write_set,
            emitted: result.emitted.clone(),
            consistency: consistency.clone(),
            trace,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("=== Execution Results ===\n");

    for (i, outcome) in result.tx_outcomes.iter().enumerate() {
        match outcome {
            tabula_core::TxOutcome::Success => println!("  tx {i}: SUCCESS"),
            tabula_core::TxOutcome::Failed {
                reason,
                partial_events,
                failed_instruction,
            } => {
                let instr = failed_instruction
                    .map(|idx| format!(" at instruction {idx}"))
                    .unwrap_or_default();
                let partial = if partial_events.is_empty() {
                    String::new()
                } else {
                    format!(", {} partial events", partial_events.len())
                };
                println!("  tx {i}: FAILED ({reason}{instr}{partial})");
            }
        }
    }
    println!();

    println!("Read set:  {} entries", read_set.len());
    println!("Write set: {} entries", write_set.len());
    println!(
        "Events:    {} total",
        trace.as_ref().map(|t| t.len()).unwrap_or(0)
    );
    println!("Emitted:   {} total", result.emitted.len());
    println!();

    println!("Write set (final state changes):");
    for cell in &write_set {
        println!(
            "  table={} row={} col={} -> {:?}",
            cell.table, cell.row, cell.col, cell.value
        );
    }
    println!();

    match &consistency {
        tabula_core::ExecutionConsistencyStatus::Passed => println!("Consistency check: PASSED"),
        tabula_core::ExecutionConsistencyStatus::Failed { reason } => {
            println!("Consistency check: FAILED ({reason})")
        }
    }

    if include_trace && let Some(trace) = &trace {
        println!("\n--- Execution Trace ---");
        for event in trace {
            let op = match event.op {
                tabula_core::OpKind::Read => "READ ",
                tabula_core::OpKind::Write => "WRITE",
            };
            println!(
                "  t={:<3} tx={} {} table={} row={} col={} -> {:?}",
                event.time,
                event.tx_index,
                op,
                event.key.table.0,
                event.key.row.0,
                event.key.col.0,
                event.value
            );
        }
    }

    Ok(())
}
