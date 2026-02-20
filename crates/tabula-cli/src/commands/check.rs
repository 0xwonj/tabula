//! Handler for the `check` subcommand.

use std::path::Path;

use crate::io::{load_program_sources, register_program};

pub fn cmd_check(program_path: &Path) -> anyhow::Result<()> {
    let (schemas, tx_types) = load_program_sources(program_path)?;
    register_program(&schemas, &tx_types)?;

    println!(
        "OK: {} table(s), {} tx type(s)",
        schemas.len(),
        tx_types.len()
    );
    Ok(())
}
