//! Handler for the `compile` subcommand.

use std::path::Path;

use tabula_driver::{load_program_sources, register_program};

use crate::io::{ProgramFile, write_json};

pub fn cmd_compile(program_path: &Path, output: Option<&Path>) -> anyhow::Result<()> {
    let sources = load_program_sources(program_path)?;

    // Validate + canonical registration via driver.
    let artifact = register_program(&sources.table_schemas, &sources.tx_types)?;

    // Determine output path
    let default_output = program_path.with_extension("json");
    let output_path = output.unwrap_or(&default_output);

    let program_file: ProgramFile = artifact.into_program_file();
    write_json(output_path, &program_file)?;

    println!(
        "Compiled {} → {}",
        program_path.display(),
        output_path.display()
    );
    Ok(())
}
