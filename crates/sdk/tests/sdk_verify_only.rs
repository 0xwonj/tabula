#![allow(missing_docs)]
#![cfg(all(feature = "verify", not(feature = "prove")))]

use tabula_compiler::{CompilerCatalogs, compile_program_source_with_catalogs};
use tabula_sdk::interop::register_compiled;
use tabula_sdk::{Context, Program, Sdk, State};

const VERIFY_ONLY_SOURCE: &str = r#"
program VerifyOnly

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    balance: u64 @ssmc;
  }
}

query read_balance(id: u64) -> u64 {
  return accounts[id].balance;
}

tx touch(id: u64) {
  if caller == 0 {
    accounts[id].balance = 1;
  } else {
    assert true;
  }
  return;
}
"#;

fn compile_artifact(sdk: &Sdk) -> tabula_sdk::Artifact {
    let compiled = compile_program_source_with_catalogs(
        VERIFY_ONLY_SOURCE,
        &CompilerCatalogs::standard().expect("standard catalogs"),
    )
    .expect("compile source");
    register_compiled(sdk, compiled).expect("register compiled program")
}

fn open_program(sdk: &Sdk) -> Program {
    let artifact = compile_artifact(sdk);
    sdk.open(artifact).expect("open artifact")
}

fn snapshot(program: &Program) -> State {
    program
        .state()
        .set("accounts", (7u64,), "balance", 11u64)
        .expect("seed balance")
        .build()
}

fn context(program: &Program, caller: u64) -> Context {
    program
        .context()
        .set("caller", caller)
        .expect("set caller")
        .build()
}

#[test]
fn verification_only_sdk_prepares_verifier_for_artifacts() {
    let sdk = Sdk::standard().expect("build standard sdk");
    let artifact = compile_artifact(&sdk);
    let program = sdk.open(artifact.clone()).expect("open artifact");
    let reopened = sdk.open(artifact).expect("reopen artifact");

    reopened.verifier().expect("prepare verifier");
    program.verifier().expect("prepare verifier");
}

#[test]
fn verification_only_sdk_keeps_query_execution_available() {
    let sdk = Sdk::standard().expect("build standard sdk");
    let program = open_program(&sdk);
    let result = program
        .runner()
        .query_symbol(
            &snapshot(&program),
            "read_balance",
            (7u64,),
            &context(&program, 1),
        )
        .expect("query execution");

    assert_eq!(result.decode_one::<u64>().expect("decode result"), 11);
}

#[test]
fn verification_only_sdk_opens_artifacts_without_compatibility_bridge() {
    let sdk = Sdk::standard().expect("build standard sdk");
    let artifact = compile_artifact(&sdk);
    let reopened = sdk.open(artifact).expect("open artifact");
    let batch = reopened
        .batch()
        .call("touch", (7u64,))
        .expect("touch batch item")
        .build();

    let execution = reopened
        .runner()
        .execute(&snapshot(&reopened), &batch, &context(&reopened, 0))
        .expect("execute batch");

    assert!(
        execution.outcomes().iter().all(|outcome| outcome.success()),
        "verify-only SDK should still execute native batches"
    );
}
