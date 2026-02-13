//! Handler for the `execute` subcommand.

use tabula_core::event::TxOutcome;
use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_core::tx::Batch;
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency;
use tabula_executor::program::Program;

use crate::io::{
    BatchFile, ExecutionOutput, ProgramFile, StateCell, StateFile, load_json, write_json,
};

pub fn cmd_execute(
    program_path: &std::path::Path,
    state_path: &std::path::Path,
    batch_path: &std::path::Path,
    output_state_path: Option<&std::path::Path>,
    include_trace: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    // Load inputs
    let program_file: ProgramFile = load_json(program_path)?;
    let state_file: StateFile = load_json(state_path)?;
    let batch_file: BatchFile = load_json(batch_path)?;

    // Build program
    let mut program = Program::new();
    for schema in &program_file.table_schemas {
        program.add_schema(schema.clone());
    }
    for def in &program_file.tx_types {
        program.register(def.clone())?;
    }

    // Build state
    let mut state = InMemoryState::new();
    for cell in &state_file.cells {
        let (key, value) = cell.to_cell_pair();
        state.set(key, value);
    }

    // Build batch
    let transactions: Vec<_> = batch_file
        .transactions
        .iter()
        .map(|t| t.to_transaction())
        .collect::<Result<_, _>>()?;
    let batch = Batch { transactions };

    // Execute
    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &MockHasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
    };
    let result = execute_batch(
        &batch,
        &program,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )?;

    // Consistency check
    let consistency = match check_consistency(&result.events, &result.read_set_old) {
        Ok(()) => "PASSED".to_string(),
        Err(e) => format!("FAILED: {e}"),
    };

    // Build output state if requested
    if let Some(out_path) = output_state_path {
        let mut new_state = state_file.cells.clone();
        // Apply write set
        for (key, value) in &result.write_set_final {
            let cell = StateCell::from_cell_pair(key, value);
            if let Some(pos) = new_state
                .iter()
                .position(|c| c.table == cell.table && c.row == cell.row && c.col == cell.col)
            {
                if value.is_none() {
                    // Deleted cell — remove from state
                    new_state.remove(pos);
                } else {
                    new_state[pos].value = cell.value;
                }
            } else if value.is_some() {
                new_state.push(cell);
            }
        }
        let new_state_file = StateFile { cells: new_state };
        write_json(out_path, &new_state_file)?;
    }

    if json_output {
        let output = ExecutionOutput {
            tx_outcomes: result.tx_outcomes,
            read_set: result
                .read_set_old
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            write_set: result
                .write_set_final
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            emitted: result.emitted,
            consistency,
            trace: if include_trace {
                Some(result.events)
            } else {
                None
            },
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Human-readable output
        println!("=== Execution Results ===\n");

        for (i, outcome) in result.tx_outcomes.iter().enumerate() {
            match outcome {
                TxOutcome::Success => println!("  tx {i}: SUCCESS"),
                TxOutcome::Failed {
                    reason,
                    partial_events,
                    failed_instruction,
                } => {
                    let instr = failed_instruction
                        .map(|i| format!(" at instruction {i}"))
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

        println!("Read set:  {} entries", result.read_set_old.len());
        println!("Write set: {} entries", result.write_set_final.len());
        println!("Events:    {} total", result.events.len());
        println!("Emitted:   {} total", result.emitted.len());
        println!();

        println!("Write set (final state changes):");
        for (key, value) in &result.write_set_final {
            println!(
                "  table={} row={} col={} -> {:?}",
                key.table.0, key.row.0, key.col.0, value
            );
        }
        println!();

        println!("Consistency check: {consistency}");

        if include_trace {
            println!("\n--- Execution Trace ---");
            for event in &result.events {
                let op = match event.op {
                    tabula_core::event::OpKind::Read => "READ ",
                    tabula_core::event::OpKind::Write => "WRITE",
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
    }

    Ok(())
}
