//! Handler for the `check` subcommand.

use std::path::Path;

use tabula_sdk::{Artifact, Sdk};

use crate::io::load_json;

pub fn cmd_check(program_path: &Path) -> anyhow::Result<()> {
    let sdk = Sdk::standard();
    let artifact = if program_path.extension().and_then(|ext| ext.to_str()) == Some("tab") {
        let source = std::fs::read_to_string(program_path)?;
        sdk.compile(&source)?
    } else {
        load_json::<Artifact>(program_path)?
    };
    let schema = artifact.schema();

    println!(
        "OK: {} table(s), {} tx entry(s)",
        schema.table_count(),
        schema.tx_count(),
    );
    Ok(())
}
