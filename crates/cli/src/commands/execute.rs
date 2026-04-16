//! `tabula execute`

use crate::app::AppContext;
use crate::cli::ExecuteArgs;
use crate::handoff::{bridge_from_receipt, encode_receipt_bridge};
use crate::io::{
    ensure_parent_dir, load_batch, load_context, load_program, load_state, write_bytes, write_json,
};
use crate::output::{execution_report, render_execution};

/// Execute one transaction batch against the supplied state snapshot.
pub(crate) fn run(ctx: &AppContext, args: &ExecuteArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk(), &args.program)?;
    let state = load_state(&args.state)?;
    let batch = load_batch(&args.batch)?;
    let context = load_context(args.context.as_deref())?;
    let receipt = loaded.program.runner().execute(&state, &batch, &context)?;

    if let Some(path) = &args.state_out {
        ensure_parent_dir(path)?;
        write_json(path, &receipt.state_after())?;
    }

    let report = execution_report(&loaded.program, &receipt, args.raw);
    if let Some(path) = &args.report_out {
        ensure_parent_dir(path)?;
        write_json(path, &report)?;
    }

    if let Some(path) = &args.receipt_out {
        ensure_parent_dir(path)?;
        let bridge = bridge_from_receipt(loaded.artifact.digest(), &receipt);
        let bytes = encode_receipt_bridge(&bridge)?;
        write_bytes(path, &bytes)?;
    }

    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_execution(&report));
    }
    Ok(())
}
