//! Handler for the `compile` subcommand.

use std::path::Path;

use crate::io::write_json;

pub fn cmd_compile(program_path: &Path, output: Option<&Path>) -> anyhow::Result<()> {
    let compiled = tabula_compiler::load_and_register_program(program_path)?;

    let default_output = program_path.with_extension("json");
    let output_path = output.unwrap_or(&default_output);

    write_json(output_path, &compiled.into_artifact())?;

    println!(
        "Compiled {} → {}",
        program_path.display(),
        output_path.display()
    );
    Ok(())
}
