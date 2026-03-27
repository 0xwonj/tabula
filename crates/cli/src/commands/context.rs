//! `tabula context ...`

use crate::app::AppContext;
use crate::cli::{ContextCommand, ContextInitArgs, ContextSetArgs};
use crate::io::{encode_json_literal, ensure_parent_dir, load_context, load_program, write_json};

/// Dispatch the `context` namespace.
pub(crate) fn run(ctx: &AppContext, command: &ContextCommand) -> anyhow::Result<()> {
    match command {
        ContextCommand::Init(args) => init(ctx, args),
        ContextCommand::Set(args) => set(ctx, args),
    }
}

fn init(ctx: &AppContext, args: &ContextInitArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let context = loaded.program.context().build();
    ensure_parent_dir(&args.out)?;
    write_json(&args.out, &context)?;
    println!("Wrote {}", args.out.display());
    Ok(())
}

fn set(ctx: &AppContext, args: &ContextSetArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let context = load_context(Some(&args.context))?;
    let field = loaded.program.context_field(&args.field)?;
    let value = encode_json_literal(&args.value, field.ty())?;
    let updated = loaded
        .program
        .context_from(&context)
        .set(&args.field, value)?
        .build();
    let output_path = args.out.as_ref().unwrap_or(&args.context);
    ensure_parent_dir(output_path)?;
    write_json(output_path, &updated)?;
    println!("Updated {}", output_path.display());
    Ok(())
}
