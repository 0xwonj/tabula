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
    let field = loaded.program.table(&args.table)?.field(&args.field)?;
    let value = encode_json_literal(&args.value, field.ty())?;
    let updated = loaded
        .program
        .state_from(&state)
        .set(&args.table, args.row, &args.field, value)?
        .build();
    let output_path = args.out.as_ref().unwrap_or(&args.state);
    ensure_parent_dir(output_path)?;
    write_json(output_path, &updated)?;
    println!("Updated {}", output_path.display());
    Ok(())
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
