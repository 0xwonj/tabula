//! `tabula prove`

use anyhow::Context as _;
use tabula_sdk::PublicStatementFile;
use tabula_sdk::interop::prepare_runtime;

use crate::app::AppContext;
use crate::cli::ProveArgs;
use crate::handoff::{decode_receipt_bridge, sdk_receipt_from_bridge};
use crate::io::{ensure_parent_dir, load_program, write_bytes, write_json};
use crate::output::{prove_output, render_prove};

/// Generate one proof from an execution receipt bridge.
pub(crate) fn run(ctx: &AppContext, args: &ProveArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk(), &args.program)?;
    let receipt_bytes = std::fs::read(&args.receipt)
        .with_context(|| format!("failed to read {}", args.receipt.display()))?;
    let bridge = decode_receipt_bridge(&receipt_bytes)?;
    let runtime = prepare_runtime(ctx.sdk(), &loaded.artifact)?;
    let receipt = sdk_receipt_from_bridge(runtime.as_ref(), bridge)?;
    let proof = loaded.program.runner().prove(&receipt)?;

    ensure_parent_dir(&args.proof_out)?;
    write_bytes(&args.proof_out, &proof.encode_binary()?)?;

    ensure_parent_dir(&args.public_statement_out)?;
    let public_statement = proof
        .public_statement()
        .expect("locally produced proof carries a public statement");
    write_json(
        &args.public_statement_out,
        &PublicStatementFile::from_public_statement(public_statement),
    )?;

    ensure_parent_dir(&args.summary_out)?;
    write_json(&args.summary_out, proof.summary())?;

    let output = prove_output(&loaded.artifact, &proof)?;
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_prove(&output));
    }
    Ok(())
}
