//! Handler for the `compile` subcommand.

use std::path::Path;

use crate::io::{ProgramFile, load_program_sources, register_program, write_json};

pub fn cmd_compile(program_path: &Path, output: Option<&Path>) -> anyhow::Result<()> {
    let (schemas, tx_types) = load_program_sources(program_path)?;

    // Validate NF
    register_program(&schemas, &tx_types)?;

    // Determine output path
    let default_output = program_path.with_extension("json");
    let output_path = output.unwrap_or(&default_output);

    let program_file = ProgramFile {
        table_schemas: schemas,
        tx_types,
    };
    write_json(output_path, &program_file)?;

    println!(
        "Compiled {} → {}",
        program_path.display(),
        output_path.display()
    );
    Ok(())
}
