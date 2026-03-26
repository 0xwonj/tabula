//! Handler for the `example` subcommand.

use tabula_sdk::Sdk;

use crate::io::write_json;

const EXAMPLE_SOURCE: &str = r#"
program Example

state {
  table balances(key id: u64) {
    amount: u64 @ssmc;
  }
}

tx set_balance(id: u64, amount: u64) {
  balances[id].amount = amount;
  return;
}
"#;

pub fn cmd_example(dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;

    let sdk = Sdk::standard();
    let artifact = sdk.compile(EXAMPLE_SOURCE)?;
    let program = sdk.open(artifact.clone())?;
    let state = program
        .state()
        .set("balances", 0, "amount", 1000u64)?
        .set("balances", 1, "amount", 500u64)?
        .build();
    let batch = program
        .batch()
        .call("set_balance", [0u64, 750u64])?
        .call("set_balance", [1u64, 625u64])?
        .build();
    let context = program.context().build();

    std::fs::write(dir.join("program.tab"), EXAMPLE_SOURCE)?;
    write_json(&dir.join("program.json"), &artifact)?;
    write_json(&dir.join("state.json"), &state)?;
    write_json(&dir.join("batch.json"), &batch)?;
    write_json(&dir.join("context.json"), &context)?;

    println!("Generated example files in {}:", dir.display());
    println!("  program.tab   - rewritten source");
    println!("  program.json  - registered native program");
    println!("  state.json    - initial state snapshot");
    println!("  batch.json    - portable entry batch");
    println!("  context.json  - public context input");
    println!();
    println!("Run with:");
    println!(
        "  tabula execute -p {dir}/program.tab -s {dir}/state.json -b {dir}/batch.json -c {dir}/context.json",
        dir = dir.display()
    );
    println!(
        "  tabula execute -p {dir}/program.json -s {dir}/state.json -b {dir}/batch.json -c {dir}/context.json",
        dir = dir.display()
    );

    Ok(())
}
