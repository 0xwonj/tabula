//! End-to-end SDK example that compiles, executes, and optionally proves
//! a small constant-product DEX program from a real `.tab` source file.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tabula_compiler::SourceCapabilityDescriptor;
use tabula_ir as ir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_sdk::{Context, Program, Sdk, State, TransactionBatch};

const PROGRAM_SOURCE: &str = include_str!("programs/dex.tab");
const PROGRAM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/programs/dex.tab");
const PAIR_ID: u64 = 0;
const INITIAL_BASE_RESERVE: u64 = 1_000_000;
const INITIAL_QUOTE_RESERVE: u64 = 2_000_000;
const FEE_BPS: u64 = 30;
const SWAP_AMOUNT_IN: u64 = 10_000;
const CALLER: u64 = 42;
const EPOCH: u64 = 7;

fn main() -> Result<(), Box<dyn Error>> {
    let output_dir = output_dir();
    fs::create_dir_all(&output_dir)?;
    fs::copy(PROGRAM_PATH, output_dir.join("program.tab"))?;

    let sdk = build_sdk()?;
    let artifact = sdk.compile(PROGRAM_SOURCE)?;
    write_json(&output_dir.join("artifact.json"), &artifact)?;

    let artifact = sdk.load_artifact(&fs::read(output_dir.join("artifact.json"))?)?;
    let program = sdk.open(artifact.clone())?;
    let runner = program.runner();

    println!(
        "Dex example source   : {}",
        Path::new(PROGRAM_PATH).display()
    );
    println!("Dex example output   : {}", output_dir.display());
    println!("Artifact digest      : {}", artifact.digest());
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
    let quoted_amount_out = runner
        .query_symbol(
            &state_before,
            "quote_exact_base_in",
            (PAIR_ID, SWAP_AMOUNT_IN),
            &context,
        )?
        .decode_one::<u64>()?;
    let configured_fee_bps = runner
        .query_symbol(&state_before, "configured_fee_bps", (PAIR_ID,), &context)?
        .decode_one::<u64>()?;
    let quoted_digest = runner
        .query_symbol(&state_before, "swap_digest", (SWAP_AMOUNT_IN,), &context)?
        .decode_one::<[u8; 32]>()?;
    let expected_quote =
        quote_exact_base_in(INITIAL_BASE_RESERVE, INITIAL_QUOTE_RESERVE, SWAP_AMOUNT_IN);
    assert_eq!(quoted_amount_out, expected_quote);
    assert_eq!(configured_fee_bps, FEE_BPS);

    let initial_base = query_reserve(&runner, &state_before, 0u64, &context)?;
    let initial_quote = query_reserve(&runner, &state_before, 1u64, &context)?;
    let initial_last_swap_out = query_last_swap_out(&runner, &state_before, &context)?;
    assert_eq!(initial_base, INITIAL_BASE_RESERVE);
    assert_eq!(initial_quote, INITIAL_QUOTE_RESERVE);
    assert_eq!(initial_last_swap_out, 0);
    assert_ne!(quoted_digest, [0u8; 32]);

    let batch = build_batch(&program, quoted_amount_out)?;

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
    assert_eq!(outcome.event_effect_count(), 1);
    assert_eq!(outcome.capability_effect_count(), 0);
    assert_eq!(outcome.relation_effect_count(), 0);
    assert_eq!(receipt.read_count(), 0);
    assert_eq!(receipt.write_count(), 1);

    let state_after = receipt.state_after();
    let final_base = query_reserve(&runner, &state_after, 0u64, &context)?;
    let final_quote = query_reserve(&runner, &state_after, 1u64, &context)?;
    let final_last_swap_out = query_last_swap_out(&runner, &state_after, &context)?;

    assert_eq!(final_base, INITIAL_BASE_RESERVE);
    assert_eq!(final_quote, INITIAL_QUOTE_RESERVE);
    assert_eq!(final_last_swap_out, quoted_amount_out);

    write_json(&output_dir.join("state_after.json"), &state_after)?;

    println!("Swap preview");
    println!("  pair            : {PAIR_ID}");
    println!("  caller          : {CALLER}");
    println!("  epoch           : {EPOCH}");
    println!("  amount_in       : {SWAP_AMOUNT_IN}");
    println!("  quoted_amount   : {quoted_amount_out}");
    println!("  configured_fee  : {configured_fee_bps} bps");
    println!("  reserve_before  : base={initial_base}, quote={initial_quote}");
    println!("  reserve_after   : base={final_base}, quote={final_quote}");
    println!("  recorded_swap   : {final_last_swap_out}");
    println!("  swap_digest     : {}", hex_bytes(&quoted_digest));
    println!(
        "  capability path : poseidon_hash imported as a capability and exercised through `swap_digest`"
    );
    println!(
        "  proof note      : the proved tx records swap settlement while quote math stays in queries"
    );

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
        "Proof generation is disabled. Re-run with `cargo run -p tabula-sdk --example dex --features prove`."
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

fn build_sdk() -> Result<Sdk, Box<dyn Error>> {
    let extension = tabula_ext::Extension::builder("poseidon")
        .add_capability(tabula_ext::Capability::new(poseidon_descriptor()))
        .build()?;
    Ok(Sdk::builder()?.with_extension(&extension)?.build()?)
}

fn poseidon_descriptor() -> SourceCapabilityDescriptor {
    SourceCapabilityDescriptor {
        path: "poseidon_hash".into(),
        inputs: vec![TYPE_U64_ID],
        outputs: vec![TYPE_BYTES32_ID],
        totality: ir::CapabilityTotality::Total,
        query_policy: ir::CapabilityQueryPolicy::QuerySafe,
        proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
        hash_family: Some(ir::HashFamily::Poseidon),
    }
}

fn build_state(program: &Program) -> Result<State, tabula_sdk::SdkError> {
    Ok(program
        .state()
        .set("pools", PAIR_ID, "reserve_base", INITIAL_BASE_RESERVE)?
        .set("pools", PAIR_ID, "reserve_quote", INITIAL_QUOTE_RESERVE)?
        .set("pools", PAIR_ID, "fee_bps", FEE_BPS)?
        .set("pools", PAIR_ID, "last_swap_out", 0u64)?
        .build())
}

fn build_context(program: &Program) -> Result<Context, tabula_sdk::SdkError> {
    Ok(program
        .context()
        .set("caller", CALLER)?
        .set("epoch", EPOCH)?
        .build())
}

fn build_batch(
    program: &Program,
    quoted_amount_out: u64,
) -> Result<TransactionBatch, tabula_sdk::SdkError> {
    Ok(program
        .batch()
        .call("swap_exact_base_for_quote", (PAIR_ID, quoted_amount_out))?
        .build())
}

fn query_reserve(
    runner: &tabula_sdk::Runner,
    state: &State,
    selector: u64,
    context: &Context,
) -> Result<u64, Box<dyn Error>> {
    Ok(runner
        .query_symbol(state, "pool_reserve", (PAIR_ID, selector), context)?
        .decode_one::<u64>()?)
}

fn quote_exact_base_in(reserve_base: u64, reserve_quote: u64, amount_in: u64) -> u64 {
    let net_amount_in = amount_in * (10_000 - FEE_BPS) / 10_000;
    net_amount_in * reserve_quote / (reserve_base + net_amount_in)
}

fn query_last_swap_out(
    runner: &tabula_sdk::Runner,
    state: &State,
    context: &Context,
) -> Result<u64, Box<dyn Error>> {
    Ok(runner
        .query_symbol(state, "last_swap_out", (PAIR_ID,), context)?
        .decode_one::<u64>()?)
}

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tabula-sdk-examples/dex")
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

fn hex_bytes(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
