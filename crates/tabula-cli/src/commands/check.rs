//! Handler for the `check` subcommand.

use std::path::Path;

use tabula_driver::{load_program_sources, register_program};

pub fn cmd_check(program_path: &Path) -> anyhow::Result<()> {
    let sources = load_program_sources(program_path)?;
    register_program(&sources.table_schemas, &sources.tx_types)?;

    println!(
        "OK: {} table(s), {} tx type(s)",
        sources.table_schemas.len(),
        sources.tx_types.len()
    );
    Ok(())
}
