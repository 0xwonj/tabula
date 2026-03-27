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
        ExampleName::Dex => data::generate_dex(&args.dir),
    }
}

mod data {
    use std::path::Path;

    use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
    use tabula_sdk::interop::{
        CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality, HashFamily,
        SdkBuilderExt, SourceCapabilityDescriptor,
    };
    use tabula_sdk::{Program, Sdk};

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
    const DEX_SOURCE: &str = include_str!("../../../sdk/examples/programs/dex.tab");

    pub(super) fn generate_basic(dir: &Path) -> anyhow::Result<()> {
        let sdk = Sdk::standard();
        let artifact = sdk.compile(BASIC_SOURCE)?;
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

        write_text(&dir.join("program.tab"), BASIC_SOURCE)?;
        write_json(&dir.join("artifact.json"), &artifact)?;
        write_json(&dir.join("state.json"), &state)?;
        write_json(&dir.join("batch.json"), &batch)?;
        write_json(&dir.join("context.json"), &context)?;

        print_summary(dir, "basic");
        Ok(())
    }

    pub(super) fn generate_membership(dir: &Path) -> anyhow::Result<()> {
        let sdk = Sdk::standard();
        let artifact = sdk.compile(MEMBERSHIP_SOURCE)?;
        let program = sdk.open(artifact.clone())?;
        let state = program.state().set("members", 1, "tier", 0u64)?.build();
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

    pub(super) fn generate_dex(dir: &Path) -> anyhow::Result<()> {
        let extension_dir = dir.join("extensions");
        std::fs::create_dir_all(&extension_dir)?;
        write_text(
            &dir.join("tabula.toml"),
            "[environment]\nextensions = [\"./extensions/poseidon.toml\"]\n",
        )?;
        write_text(&extension_dir.join("poseidon.toml"), DEX_EXTENSION_BUNDLE)?;

        let sdk = Sdk::builder()
            .with_capability_descriptor(poseidon_descriptor())?
            .build()?;
        let artifact = sdk.compile(DEX_SOURCE)?;
        let program = sdk.open(artifact.clone())?;
        let state = dex_state(&program)?;
        let batch = program
            .batch()
            .call("swap_exact_base_for_quote", (0u64, 19_743u64))?
            .build();
        let context = program
            .context()
            .set("caller", 42u64)?
            .set("epoch", 7u64)?
            .build();

        write_text(&dir.join("program.tab"), DEX_SOURCE)?;
        write_json(&dir.join("artifact.json"), &artifact)?;
        write_json(&dir.join("state.json"), &state)?;
        write_json(&dir.join("batch.json"), &batch)?;
        write_json(&dir.join("context.json"), &context)?;

        print_summary(dir, "dex");
        println!("This example includes tabula.toml and a declarative capability bundle.");
        Ok(())
    }

    fn dex_state(program: &Program) -> anyhow::Result<tabula_sdk::State> {
        Ok(program
            .state()
            .set("pools", 0, "reserve_base", 1_000_000u64)?
            .set("pools", 0, "reserve_quote", 2_000_000u64)?
            .set("pools", 0, "fee_bps", 30u64)?
            .set("pools", 0, "last_swap_out", 0u64)?
            .build())
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
            "  tabula execute --program {dir}/program.tab --state {dir}/state.json --batch {dir}/batch.json --context {dir}/context.json",
            dir = dir.display()
        );
    }

    fn poseidon_descriptor() -> SourceCapabilityDescriptor {
        SourceCapabilityDescriptor {
            path: "poseidon_hash".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_BYTES32_ID],
            totality: CapabilityTotality::Total,
            query_policy: CapabilityQueryPolicy::QuerySafe,
            proof_visibility: CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: Some(HashFamily::Poseidon),
        }
    }

    const DEX_EXTENSION_BUNDLE: &str = r#"
version = 1
name = "poseidon"

[[capabilities]]
path = "poseidon_hash"
inputs = ["u64"]
outputs = ["bytes32"]
totality = "total"
query_policy = "query_safe"
proof_visibility = "opaque_runtime_only"
hash_family = "poseidon"
"#;
}
