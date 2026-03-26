//! Handler for the `execute` subcommand.

use tabula_sdk::{Artifact, Context, Program, Sdk, State, TransactionBatch};

use crate::io::{ExecutionOutput, load_json, write_json};

fn load_program(sdk: &Sdk, program_path: &std::path::Path) -> anyhow::Result<Program> {
    if program_path.extension().and_then(|ext| ext.to_str()) == Some("tab") {
        let source = std::fs::read_to_string(program_path)?;
        let artifact = sdk.compile(&source)?;
        return Ok(sdk.open(artifact)?);
    }

    let artifact: Artifact = load_json(program_path)?;
    Ok(sdk.open(artifact)?)
}

pub fn cmd_execute(
    program_path: &std::path::Path,
    state_path: &std::path::Path,
    batch_path: &std::path::Path,
    context_path: Option<&std::path::Path>,
    output_state_path: Option<&std::path::Path>,
    json_output: bool,
) -> anyhow::Result<()> {
    let sdk = Sdk::standard();
    let program = load_program(&sdk, program_path)?;
    let runner = program.runner();

    let snapshot: State = load_json(state_path)?;
    let batch: TransactionBatch = load_json(batch_path)?;
    let context: Context = match context_path {
        Some(path) => load_json(path)?,
        None => Context::default(),
    };

    let execution = runner.execute(&snapshot, &batch, &context)?;
    let outcomes = execution.outcomes();

    if let Some(out_path) = output_state_path {
        write_json(out_path, &execution.state_after())?;
    }

    let tx_outcomes = outcomes
        .iter()
        .map(|outcome| {
            if outcome.success() {
                serde_json::json!({
                    "status": "success",
                    "tx_index": outcome.tx_index(),
                    "entry_id": outcome.entry_id().0,
                    "state_effect_count": outcome.state_effect_count(),
                    "event_effect_count": outcome.event_effect_count(),
                    "capability_effect_count": outcome.capability_effect_count(),
                    "relation_effect_count": outcome.relation_effect_count(),
                })
            } else {
                serde_json::json!({
                    "status": "failed",
                    "tx_index": outcome.tx_index(),
                    "entry_id": outcome.entry_id().0,
                    "reason": outcome.reason(),
                    "failed_op_index": outcome.failed_op_index(),
                })
            }
        })
        .collect::<Vec<_>>();

    if json_output {
        let output = ExecutionOutput {
            tx_outcomes,
            read_count: execution.read_count(),
            write_count: execution.write_count(),
            state_after: execution.state_after(),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("=== Execution Results ===\n");
    for outcome in &outcomes {
        if outcome.success() {
            println!(
                "  tx {}: SUCCESS (state effects={}, event effects={})",
                outcome.tx_index(),
                outcome.state_effect_count(),
                outcome.event_effect_count(),
            );
        } else {
            let op = outcome
                .failed_op_index()
                .map(|value| format!(" at op {value}"))
                .unwrap_or_default();
            println!(
                "  tx {}: FAILED ({}{op})",
                outcome.tx_index(),
                outcome.reason().unwrap_or("unknown failure"),
            );
        }
    }
    println!();
    println!("Read set:  {} entries", execution.read_count());
    println!("Write set: {} entries", execution.write_count());
    println!("Final state:");
    for (key, value) in execution.state_after().cells() {
        println!(
            "  table={} row={} field={} = {:?}",
            key.table.0, key.row.0, key.col.0, value
        );
    }

    Ok(())
}
