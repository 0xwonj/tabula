//! Handler for the `compile` subcommand.

use std::path::Path;

use tabula_sdk::Sdk;

use crate::io::write_json;

pub fn cmd_compile(program_path: &Path, output: Option<&Path>) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(program_path)?;
    let sdk = Sdk::standard();
    let artifact = sdk.compile(&source)?;

    let default_output = program_path.with_extension("json");
    let output_path = output.unwrap_or(&default_output);
    write_json(output_path, &artifact)?;

    println!(
        "Compiled {} -> {}",
        program_path.display(),
        output_path.display()
    );
    Ok(())
}
