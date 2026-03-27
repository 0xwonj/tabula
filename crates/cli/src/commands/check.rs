//! `tabula check`

use crate::app::AppContext;
use crate::cli::CheckArgs;
use crate::io::load_artifact;
use crate::output::{check_output, render_check};

/// Validate source or artifact and print a schema-aware summary.
pub(crate) fn run(ctx: &AppContext, args: &CheckArgs) -> anyhow::Result<()> {
    let (artifact, input_kind) = load_artifact(ctx.sdk()?, &args.program)?;
    let output = check_output(&artifact, input_kind);
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_check(&output));
    }
    Ok(())
}
