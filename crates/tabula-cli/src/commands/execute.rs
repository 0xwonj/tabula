//! Handler for the `execute` subcommand.

use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_core::{Batch, CellKey, ExecutionConsistencyStatus, TxOutcome, Value};
use tabula_driver::load_and_register_program;
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;

use crate::io::{BatchFile, ExecutionOutput, StateCell, StateFile, load_json, write_json};

pub fn cmd_execute(
    program_path: &std::path::Path,
    state_path: &std::path::Path,
    batch_path: &std::path::Path,
    output_state_path: Option<&std::path::Path>,
    include_trace: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    // Load inputs
    let artifact = load_and_register_program(program_path)?;
    let state_file: StateFile = load_json(state_path)?;
    let batch_file: BatchFile = load_json(batch_path)?;

    // Build state
    let mut state = InMemoryState::new();
    for cell in &state_file.cells {
        let (key, value) = cell.to_cell_pair()?;
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
        &artifact.program,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )?;

    // Consistency check
    let consistency = check_consistency_status(&result.events, &result.read_set_old);

    // Build output state if requested
    if let Some(out_path) = output_state_path {
        let new_state = merge_output_state_cells(&state_file.cells, &result.write_set_final);
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
            consistency: consistency.clone(),
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

        match consistency {
            ExecutionConsistencyStatus::Passed => println!("Consistency check: PASSED"),
            ExecutionConsistencyStatus::Failed { ref reason } => {
                println!("Consistency check: FAILED ({reason})")
            }
        }

        if include_trace {
            println!("\n--- Execution Trace ---");
            for event in &result.events {
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
    }

    Ok(())
}

fn merge_output_state_cells(
    initial_cells: &[StateCell],
    write_set_final: &[(CellKey, Option<Value>)],
) -> Vec<StateCell> {
    let mut merged: std::collections::BTreeMap<(u32, u64, u16), Value> =
        std::collections::BTreeMap::new();

    // Keep only one value per logical key (last one in input wins), mirroring
    // the semantics of loading state into InMemoryState.
    for cell in initial_cells {
        if let Some(value) = cell.value {
            merged.insert((cell.table, cell.row, cell.col), value);
        }
    }

    for (key, value) in write_set_final {
        let tuple_key = (key.table.0, key.row.0, key.col.0);
        match value {
            Some(v) => {
                merged.insert(tuple_key, *v);
            }
            None => {
                merged.remove(&tuple_key);
            }
        }
    }

    merged
        .into_iter()
        .map(|((table, row, col), value)| StateCell {
            table,
            row,
            col,
            value: Some(value),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::{ColId, RowKey, TableId};

    #[test]
    fn merge_output_state_cells_deduplicates_initial_cells() {
        let initial = vec![
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(10)),
            },
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(20)),
            },
        ];

        let merged = merge_output_state_cells(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(20)));
    }

    #[test]
    fn merge_output_state_cells_applies_updates_and_deletes() {
        let initial = vec![
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(10)),
            },
            StateCell {
                table: 0,
                row: 2,
                col: 2,
                value: Some(Value::U64(99)),
            },
        ];

        let writes = vec![
            (
                CellKey {
                    table: TableId(0),
                    row: RowKey(1),
                    col: ColId(2),
                },
                Some(Value::U64(77)),
            ),
            (
                CellKey {
                    table: TableId(0),
                    row: RowKey(2),
                    col: ColId(2),
                },
                None,
            ),
        ];

        let merged = merge_output_state_cells(&initial, &writes);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].table, 0);
        assert_eq!(merged[0].row, 1);
        assert_eq!(merged[0].col, 2);
        assert_eq!(merged[0].value, Some(Value::U64(77)));
    }
}
