//! Handler for the `check` subcommand.

use std::path::Path;

pub fn cmd_check(program_path: &Path) -> anyhow::Result<()> {
    let compiled = tabula_compiler::load_and_register_program(program_path)?;

    println!(
        "OK: {} table(s), {} tx type(s)",
        compiled.table_schemas().len(),
        compiled.tx_types().len()
    );
    Ok(())
}
