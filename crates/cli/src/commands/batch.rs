//! `tabula batch ...`

use crate::app::AppContext;
use crate::cli::{BatchCallArgs, BatchCommand, BatchInitArgs};
use crate::io::{encode_json_args, ensure_parent_dir, load_batch, load_program, write_json};

/// Dispatch the `batch` namespace.
pub(crate) fn run(ctx: &AppContext, command: &BatchCommand) -> anyhow::Result<()> {
    match command {
        BatchCommand::Init(args) => init(ctx, args),
        BatchCommand::Call(args) => call(ctx, args),
    }
}

fn init(_ctx: &AppContext, args: &BatchInitArgs) -> anyhow::Result<()> {
    ensure_parent_dir(&args.out)?;
    write_json(&args.out, &tabula_sdk::TransactionBatch::default())?;
    println!("Wrote {}", args.out.display());
    Ok(())
}

fn call(ctx: &AppContext, args: &BatchCallArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk()?, &args.program)?;
    let batch = load_batch(&args.batch)?;
    let tx = loaded.program.tx(&args.tx)?;
    let params = encode_json_args(
        &args.args,
        &tx.params()
            .iter()
            .map(tabula_sdk::ParameterHandle::ty)
            .collect::<Vec<_>>(),
    )?;
    let updated = loaded
        .program
        .batch_from(&batch)
        .call(&args.tx, params)?
        .build();
    let output_path = args.out.as_ref().unwrap_or(&args.batch);
    ensure_parent_dir(output_path)?;
    write_json(output_path, &updated)?;
    println!("Updated {}", output_path.display());
    Ok(())
}
