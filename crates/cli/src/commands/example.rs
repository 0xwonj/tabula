//! `tabula example`

use anyhow::Context as _;

use crate::app::AppContext;
use crate::cli::{ExampleArgs, ExampleName};

/// Generate a ready-to-run example directory.
pub(crate) fn run(_ctx: &AppContext, args: &ExampleArgs) -> anyhow::Result<()> {
    std::fs::create_dir_all(&args.dir)
        .with_context(|| format!("failed to create example directory {}", args.dir.display()))?;
    match args.name {
        ExampleName::Basic => data::generate_basic(&args.dir),
        ExampleName::Membership => data::generate_membership(&args.dir),
    }
}

mod data {
    use std::path::Path;

    use tabula_sdk::Sdk;

    use crate::io::{write_json, write_text};

    const BASIC_SOURCE: &str = r#"
program Example

state {
  table balances(key id: u64) {
    amount: u64 @ssmc;
  }
}

query balance_of(id: u64) -> u64 {
  return balances[id].amount;
}

tx set_balance(id: u64, amount: u64) {
  balances[id].amount = amount;
  return;
}
"#;

    const MEMBERSHIP_SOURCE: &str = include_str!("../../../sdk/examples/programs/membership.tab");

    pub(super) fn generate_basic(dir: &Path) -> anyhow::Result<()> {
        let sdk = Sdk::standard()?;
        let artifact = sdk.compile(BASIC_SOURCE)?;
        let program = sdk.open(artifact.clone())?;
        let state = program
            .state()
            .set("balances", (0u64,), "amount", 1000u64)?
            .set("balances", (1u64,), "amount", 500u64)?
            .build();
        let batch = program
            .batch()
            .call("set_balance", [0u64, 750u64])?
            .call("set_balance", [1u64, 625u64])?
            .build();
        let context = program.context().build();

        write_text(&dir.join("program.tab"), BASIC_SOURCE)?;
        write_json(&dir.join("artifact.json"), &artifact)?;
        write_json(&dir.join("state.json"), &state)?;
        write_json(&dir.join("batch.json"), &batch)?;
        write_json(&dir.join("context.json"), &context)?;

        print_summary(dir, "basic");
        Ok(())
    }

    pub(super) fn generate_membership(dir: &Path) -> anyhow::Result<()> {
        let sdk = Sdk::standard()?;
        let artifact = sdk.compile(MEMBERSHIP_SOURCE)?;
        let program = sdk.open(artifact.clone())?;
        let state = program
            .state()
            .set("members", (1u64,), "tier", 0u64)?
            .build();
        let batch = program.batch().call("approve_upgrade", (1u64,))?.build();
        let context = program
            .context()
            .set("caller", 7u64)?
            .set("epoch", 11u64)?
            .build();

        write_text(&dir.join("program.tab"), MEMBERSHIP_SOURCE)?;
        write_json(&dir.join("artifact.json"), &artifact)?;
        write_json(&dir.join("state.json"), &state)?;
        write_json(&dir.join("batch.json"), &batch)?;
        write_json(&dir.join("context.json"), &context)?;

        print_summary(dir, "membership");
        Ok(())
    }

    fn print_summary(dir: &Path, name: &str) {
        println!("Generated {name} example in {}", dir.display());
        println!("  program.tab");
        println!("  artifact.json");
        println!("  state.json");
        println!("  batch.json");
        println!("  context.json");
        println!("Run with:");
        println!(
            "  target/debug/tabula-cli execute --program {dir}/program.tab --state {dir}/state.json --batch {dir}/batch.json --context {dir}/context.json",
            dir = dir.display()
        );
    }
}
