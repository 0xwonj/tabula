//! `tabula compile`

use anyhow::Context as _;

use crate::app::AppContext;
use crate::cli::CompileArgs;
use crate::io::{default_artifact_output, ensure_parent_dir, load_artifact, write_json};

/// Compile source into one artifact JSON file.
pub(crate) fn run(ctx: &AppContext, args: &CompileArgs) -> anyhow::Result<()> {
    let (artifact, _) = load_artifact(ctx.sdk(), &args.program)?;
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| default_artifact_output(&args.program));
    ensure_parent_dir(&output_path)?;
    write_json(&output_path, &artifact)
        .with_context(|| format!("failed to write artifact {}", output_path.display()))?;
    println!(
        "Compiled {} -> {}",
        args.program.display(),
        output_path.display()
    );
    Ok(())
}
