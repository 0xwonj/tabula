//! Handler for the `check` subcommand.

use std::path::Path;

pub fn cmd_check(program_path: &Path) -> anyhow::Result<()> {
    let registered = tabula_driver::load_and_register_program(program_path)?;

    println!(
        "OK: {} table(s), {} tx type(s)",
        registered.table_schemas.len(),
        registered.tx_types.len()
    );
    Ok(())
}
