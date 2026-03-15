//! Handler for the `example` subcommand.

use tabula_compiler::transfer_example_bundle;

use crate::io::write_json;

pub fn cmd_example(dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let bundle = transfer_example_bundle()?;

    // Write .tab source
    std::fs::write(dir.join("program.tab"), bundle.program_tab_source)
        .map_err(|e| anyhow::anyhow!("failed to write program.tab: {e}"))?;

    write_json(&dir.join("program.json"), &bundle.program)?;
    write_json(&dir.join("state.json"), &bundle.state)?;
    write_json(&dir.join("batch.json"), &bundle.batch)?;

    println!("Generated example files in {}:", dir.display());
    println!("  program.tab   - DSL source");
    println!("  program.json  - compiled IR");
    println!("  state.json    - 3 accounts (1000, 500, 200)");
    println!("  batch.json    - 3 transfers");
    println!();
    println!("Run with:");
    println!(
        "  tabula execute -p {dir}/program.tab -s {dir}/state.json -b {dir}/batch.json",
        dir = dir.display()
    );
    println!(
        "  tabula execute -p {dir}/program.json -s {dir}/state.json -b {dir}/batch.json",
        dir = dir.display()
    );

    Ok(())
}
