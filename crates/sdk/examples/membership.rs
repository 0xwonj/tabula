//! End-to-end SDK example for a small membership approval flow.
//! It demonstrates compile/load/open, symbol-first builders, queries,
//! a stateful transaction, and optional proof generation.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tabula_sdk::{Context, Program, Sdk, State, TransactionBatch};

const PROGRAM_SOURCE: &str = include_str!("programs/membership.tab");
const PROGRAM_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/programs/membership.tab"
);
const MEMBER_ID: u64 = 1;
const INITIAL_TIER: u64 = 0;
const EXPECTED_TIER_AFTER_APPROVAL: u64 = 1;
const CALLER: u64 = 7;
const EPOCH: u64 = 11;

fn main() -> Result<(), Box<dyn Error>> {
    let output_dir = output_dir();
    fs::create_dir_all(&output_dir)?;
    fs::copy(PROGRAM_PATH, output_dir.join("program.tab"))?;

    let sdk = Sdk::standard()?;
    let artifact = sdk.compile(PROGRAM_SOURCE)?;
    write_json(&output_dir.join("artifact.json"), &artifact)?;

    let artifact = sdk.load_artifact(&fs::read(output_dir.join("artifact.json"))?)?;
    let program = sdk.open(artifact.clone())?;
    let runner = program.runner();

    println!(
        "Membership example source : {}",
        Path::new(PROGRAM_PATH).display()
    );
    println!("Membership example output : {}", output_dir.display());
    println!("Artifact digest           : {}", artifact.digest());
    println!("Schema summary");
    println!(
        "  tables  : {}",
        join_symbols(
            program
                .schema()
                .tables()
                .iter()
                .map(tabula_sdk::TableHandle::symbol)
        )
    );
    println!(
        "  txs     : {}",
        join_symbols(
            program
                .schema()
                .txs()
                .iter()
                .map(tabula_sdk::TxHandle::symbol)
        )
    );
    println!(
        "  queries : {}",
        join_symbols(
            program
                .schema()
                .queries()
                .iter()
                .map(tabula_sdk::QueryHandle::symbol)
        )
    );

    let state_before = build_state(&program)?;
    let context = build_context(&program)?;

    let tier_before = current_tier(&runner, &state_before, &context)?;
    let preview = preview_upgrade(&runner, &state_before, &context)?;
    assert_eq!(tier_before, INITIAL_TIER);
    assert_eq!(preview, EXPECTED_TIER_AFTER_APPROVAL);

    let batch = build_batch(&program)?;
    write_json(&output_dir.join("state_before.json"), &state_before)?;
    write_json(&output_dir.join("batch.json"), &batch)?;
    write_json(&output_dir.join("context.json"), &context)?;

    let state_before: State = read_json(&output_dir.join("state_before.json"))?;
    let batch: TransactionBatch = read_json(&output_dir.join("batch.json"))?;
    let context: Context = read_json(&output_dir.join("context.json"))?;

    #[cfg(feature = "prove")]
    let (receipt, proof) = runner.execute_and_prove(&state_before, &batch, &context)?;
    #[cfg(not(feature = "prove"))]
    let receipt = runner.execute(&state_before, &batch, &context)?;

    let outcomes = receipt.outcomes();
    assert_eq!(outcomes.len(), 1);
    let outcome = &outcomes[0];
    assert!(outcome.success());
    assert!(outcome.state_effect_count() >= 1);
    assert_eq!(outcome.event_effect_count(), 1);
    assert!(outcome.relation_effect_count() >= 3);

    let state_after = receipt.state_after();
    let tier_after = current_tier(&runner, &state_after, &context)?;
    assert_eq!(tier_after, EXPECTED_TIER_AFTER_APPROVAL);
    write_json(&output_dir.join("state_after.json"), &state_after)?;

    println!("Approval summary");
    println!("  member          : {MEMBER_ID}");
    println!("  caller          : {CALLER}");
    println!("  epoch           : {EPOCH}");
    println!("  tier_before     : {tier_before}");
    println!("  preview_upgrade : {preview}");
    println!("  tier_after      : {tier_after}");
    println!("  read_count      : {}", receipt.read_count());
    println!("  write_count     : {}", receipt.write_count());

    #[cfg(feature = "prove")]
    {
        write_json(&output_dir.join("statement.json"), proof.statement())?;
        write_json(&output_dir.join("proof_summary.json"), proof.summary())?;
        program.verifier().verify(&proof)?;
        sdk.open(artifact)?.verifier().verify(&proof)?;
        println!("Proof summary");
        println!("  chip_count      : {}", proof.summary().chip_count);
        println!(
            "  statement_path  : {}",
            output_dir.join("statement.json").display()
        );
        println!(
            "  summary_path    : {}",
            output_dir.join("proof_summary.json").display()
        );
    }

    #[cfg(not(feature = "prove"))]
    println!(
        "Proof generation is disabled. Re-run with `cargo run -p tabula-sdk --example membership --features prove`."
    );

    println!("Wrote:");
    println!("  {}", output_dir.join("program.tab").display());
    println!("  {}", output_dir.join("artifact.json").display());
    println!("  {}", output_dir.join("state_before.json").display());
    println!("  {}", output_dir.join("batch.json").display());
    println!("  {}", output_dir.join("context.json").display());
    println!("  {}", output_dir.join("state_after.json").display());

    Ok(())
}

fn build_state(program: &Program) -> Result<State, tabula_sdk::SdkError> {
    Ok(program
        .state()
        .set("members", MEMBER_ID, "tier", INITIAL_TIER)?
        .build())
}

fn build_context(program: &Program) -> Result<Context, tabula_sdk::SdkError> {
    Ok(program
        .context()
        .set("caller", CALLER)?
        .set("epoch", EPOCH)?
        .build())
}

fn build_batch(program: &Program) -> Result<TransactionBatch, tabula_sdk::SdkError> {
    Ok(program
        .batch()
        .call("approve_upgrade", (MEMBER_ID,))?
        .build())
}

fn current_tier(
    runner: &tabula_sdk::Runner,
    state: &State,
    context: &Context,
) -> Result<u64, Box<dyn Error>> {
    Ok(runner
        .query_symbol(state, "current_tier", (MEMBER_ID,), context)?
        .decode_one::<u64>()?)
}

fn preview_upgrade(
    runner: &tabula_sdk::Runner,
    state: &State,
    context: &Context,
) -> Result<u64, Box<dyn Error>> {
    Ok(runner
        .query_symbol(state, "preview_upgrade", (MEMBER_ID,), context)?
        .decode_one::<u64>()?)
}

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tabula-sdk-examples/membership")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn join_symbols<'a>(symbols: impl Iterator<Item = &'a str>) -> String {
    let names = symbols.collect::<Vec<_>>();
    if names.is_empty() {
        "(none)".into()
    } else {
        names.join(", ")
    }
}
