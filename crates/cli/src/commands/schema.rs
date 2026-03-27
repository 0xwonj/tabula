//! `tabula schema`

use crate::app::AppContext;
use crate::cli::SchemaArgs;
use crate::io::load_artifact;
use crate::output::{render_schema, schema_output};

/// Print the full static schema of a source or artifact.
pub(crate) fn run(ctx: &AppContext, args: &SchemaArgs) -> anyhow::Result<()> {
    let (artifact, _) = load_artifact(ctx.sdk()?, &args.program)?;
    let output = schema_output(&artifact);
    if ctx.wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_schema(&output));
    }
    Ok(())
}
