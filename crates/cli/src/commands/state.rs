//! `tabula state ...`

use crate::app::AppContext;
use crate::cli::{StateCommand, StateInitArgs, StateInspectArgs, StateSetArgs};
use crate::io::{encode_json_literal, ensure_parent_dir, load_program, load_state, write_json};
use crate::output::{render_state, state_output};

/// Dispatch the `state` namespace.
pub(crate) fn run(ctx: &AppContext, command: &StateCommand) -> anyhow::Result<()> {
    match command {
        StateCommand::Init(args) => init(ctx, args),
        StateCommand::Set(args) => set(ctx, args),
        StateCommand::Inspect(args) => inspect(ctx, args),
    }
}

fn init(ctx: &AppContext, args: &StateInitArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let state = loaded.program.state().build();
    ensure_parent_dir(&args.out)?;
    write_json(&args.out, &state)?;
    println!("Wrote {}", args.out.display());
    Ok(())
}

fn set(ctx: &AppContext, args: &StateSetArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let state = load_state(&args.state)?;
    let table = loaded.program.table(&args.table)?;
    let field = table.field(&args.field)?;
    let key = encode_key_literal(&args.key, table.key_components())?;
    let value = encode_json_literal(&args.value, field.ty())?;
    let updated = loaded
        .program
        .state_from(&state)
        .set(&args.table, key, &args.field, value)?
        .build();
    let output_path = args.out.as_ref().unwrap_or(&args.state);
    ensure_parent_dir(output_path)?;
    write_json(output_path, &updated)?;
    println!("Updated {}", output_path.display());
    Ok(())
}

fn encode_key_literal(
    raw: &str,
    expected: &[tabula_sdk::KeyComponentHandle],
) -> anyhow::Result<Vec<tabula_sdk::interop::PortableValue>> {
    let parsed: serde_json::Value = serde_json::from_str(raw)?;
    let values = parsed
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("--key must be a JSON array"))?;
    if values.len() != expected.len() {
        return Err(anyhow::anyhow!(
            "table key expects {} components but {} were provided",
            expected.len(),
            values.len()
        ));
    }
    values
        .iter()
        .zip(expected.iter())
        .map(
            |(value, component): (&serde_json::Value, &tabula_sdk::KeyComponentHandle)| {
                encode_json_literal(&value.to_string(), component.ty())
            },
        )
        .collect()
}

fn inspect(ctx: &AppContext, args: &StateInspectArgs) -> anyhow::Result<()> {
    let state = load_state(&args.state)?;
    let program = match args.program.as_deref() {
        Some(path) => Some(load_program(ctx.sdk()?, path)?.program),
        None => None,
    };
    let output = state_output(program.as_ref(), &state, args.table.as_deref(), args.raw);
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_state(&output));
    }
    Ok(())
}
