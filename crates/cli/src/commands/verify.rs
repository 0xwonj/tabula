//! `tabula verify`

use anyhow::Context as _;
use tabula_sdk::PublicStatementFile;

use crate::app::AppContext;
use crate::cli::VerifyArgs;
use crate::io::load_program;
use crate::output::{render_verify, verify_output};

/// Verify one canonical `proof.bin` against a selected program artifact.
pub(crate) fn run(ctx: &AppContext, args: &VerifyArgs) -> anyhow::Result<()> {
    let loaded = load_program(ctx.sdk(), &args.program)?;
    let bytes = std::fs::read(&args.proof)
        .with_context(|| format!("failed to read {}", args.proof.display()))?;
    let proof = tabula_sdk::Proof::decode_binary(&bytes)?;
    let statement_bytes = std::fs::read(&args.statement)
        .with_context(|| format!("failed to read {}", args.statement.display()))?;
    let statement = PublicStatementFile::from_json_bytes(&statement_bytes)
        .with_context(|| format!("failed to decode {}", args.statement.display()))?
        .to_public_statement()
        .with_context(|| format!("failed to validate {}", args.statement.display()))?;
    loaded
        .program
        .verifier()?
        .verify_public_statement(&proof, &statement)?;

    let output = verify_output(&loaded.artifact, &proof)?;
    if AppContext::wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_verify(&output));
    }
    Ok(())
}
