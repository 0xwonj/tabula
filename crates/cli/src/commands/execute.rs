//! Handler for the `execute` subcommand.

use tabula_artifact::{BatchFile, StateFile};
use tabula_core::mock::Blake3Hasher;
use tabula_driver::{BatchInput, run_batch};

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

    // 3. Execute via driver pipeline
    let executed = run_batch(&BatchInput {
        program: &registered.program,
        state: &state_file,
        batch: &batch_file,
        hasher: &Blake3Hasher,
    })?;

    if let Some(out_path) = output_state_path {
        write_json(out_path, &executed.state_after)?;
    }

    let read_set: Vec<StateCell> = executed
        .read_set
        .iter()
        .map(|(k, v)| StateCell::from_cell_pair(k, v))
        .collect();
    let write_set: Vec<StateCell> = executed
        .write_set
        .iter()
        .map(|(k, v)| StateCell::from_cell_pair(k, v))
        .collect();
    let all_events: Vec<_> = executed
        .txs
        .iter()
        .flat_map(|tx| tx.access_trace())
        .cloned()
        .collect();
    let trace = if include_trace {
        Some(all_events)
    } else {
        None
    };

    let emitted: Vec<_> = executed
        .txs
        .iter()
        .filter_map(|tx| match tx {
            tabula_core::TxResult::Success { emitted, .. } => Some(emitted.iter()),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect();

    if json_output {
        let output = ExecutionOutput {
            tx_results: executed.txs.clone(),
            read_set,
            write_set,
            emitted: emitted.clone(),
            consistency: executed.consistency.clone(),
            trace,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("=== Execution Results ===\n");

    for (i, tx_result) in executed.txs.iter().enumerate() {
        match tx_result {
            tabula_core::TxResult::Success { .. } => println!("  tx {i}: SUCCESS"),
            tabula_core::TxResult::Failed {
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
        trace.as_ref().map_or(0, std::vec::Vec::len)
    );
    println!("Emitted:   {} total", emitted.len());
    println!();

    println!("Write set (final state changes):");
    for cell in &write_set {
        println!(
            "  table={} row={} col={} -> {:?}",
            cell.table, cell.row, cell.col, cell.value
        );
    }
    println!();

    match &executed.consistency {
        tabula_core::ExecutionConsistencyStatus::Passed => println!("Consistency check: PASSED"),
        tabula_core::ExecutionConsistencyStatus::Failed { reason } => {
            println!("Consistency check: FAILED ({reason})");
        }
    }

    if include_trace {
        println!("\n--- Execution Trace ---");
        for (tx_idx, tx) in executed.txs.iter().enumerate() {
            if let tabula_core::TxResult::Success { access_trace, .. } = tx {
                for event in access_trace {
                    let op = match event.op {
                        tabula_core::OpKind::Read => "READ ",
                        tabula_core::OpKind::Write => "WRITE",
                    };
                    println!(
                        "  t={:<3} tx={} {} table={} row={} col={} -> {:?}",
                        event.time,
                        tx_idx,
                        op,
                        event.key.table.0,
                        event.key.row.0,
                        event.key.col.0,
                        event.value
                    );
                }
            }
        }
    }

    Ok(())
}
