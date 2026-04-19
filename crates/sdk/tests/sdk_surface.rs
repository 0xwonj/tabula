#![allow(missing_docs)]

use std::collections::BTreeMap;

use serde_json::json;
#[cfg(not(feature = "compile"))]
use tabula_compiler::compile_program_source_with_catalogs;
use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_sdk::interop::register_compiled;
use tabula_sdk::{Context, DecodeValue, Program, Sdk, State};
use tabula_types::u64_portable;

const SDK_SURFACE_SOURCE: &str = r#"
use capability poseidon_hash;

program NativeProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table users(key id: u64) {
    tier: u64 @ssmc;
    seen: u64 @ssmc;
  }
}

event Registered(id: u64, actor: u64);

query choose(flag: bool, seed: u64) -> u64 {
  if flag {
    assert true;
  } else {
    assert true;
  }
  match seed {
    0 => {
      assert true;
    }
    _ => {
      assert true;
    }
  }
  return select(flag, caller, seed);
}

tx register(flag: bool, id: u64) {
  let digest = poseidon_hash(id);
  if flag {
    users[id].tier = caller;
  } else {
    assert true;
  }
  match id {
    0 => {
      users[id].seen = 1;
    }
    _ => {
      emit Registered(id, caller);
    }
  }
  return;
}
"#;

#[cfg(feature = "prove")]
const SDK_SURFACE_ALT_SCHEME_SOURCE: &str = r#"
use capability poseidon_hash;

program NativeProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table users(key id: u64) {
    tier: u64 @smt;
    seen: u64 @ssmc;
  }
}

event Registered(id: u64, actor: u64);

query choose(flag: bool, seed: u64) -> u64 {
  if flag {
    assert true;
  } else {
    assert true;
  }
  match seed {
    0 => {
      assert true;
    }
    _ => {
      assert true;
    }
  }
  return select(flag, caller, seed);
}

tx register(flag: bool, id: u64) {
  let digest = poseidon_hash(id);
  if flag {
    users[id].tier = caller;
  } else {
    assert true;
  }
  match id {
    0 => {
      users[id].seen = 1;
    }
    _ => {
      emit Registered(id, caller);
    }
  }
  return;
}
"#;

fn state_values(state: &State) -> BTreeMap<(u32, u64, u16), PortableValue> {
    state
        .cells()
        .map(|cell| {
            let [key_component] = cell.key.as_slice() else {
                panic!("expected unary logical key in sdk surface test");
            };
            let key_id = u64::decode_from(key_component).expect("decode logical key component");
            ((cell.table.0, key_id, cell.field.0), cell.value.clone())
        })
        .collect()
}

fn sdk() -> Sdk {
    let extension = tabula_ext::Extension::builder("poseidon")
        .add_capability(tabula_ext::Capability::new(poseidon_descriptor()))
        .build()
        .expect("build poseidon extension");
    Sdk::builder()
        .expect("create sdk builder")
        .with_extension(&extension)
        .expect("install poseidon extension")
        .build()
        .expect("build sdk")
}

fn poseidon_descriptor() -> tabula_compiler::SourceCapabilityDescriptor {
    tabula_compiler::SourceCapabilityDescriptor {
        path: "poseidon_hash".into(),
        inputs: vec![TYPE_U64_ID],
        outputs: vec![TYPE_BYTES32_ID],
        totality: ir::CapabilityTotality::Total,
        query_policy: ir::CapabilityQueryPolicy::QuerySafe,
        proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
        hash_family: Some(ir::HashFamily::Poseidon),
    }
}

fn compiler_catalogs() -> tabula_compiler::CompilerCatalogs {
    tabula_compiler::CompilerCatalogs::standard()
        .expect("standard catalogs")
        .with_capability_descriptor(poseidon_descriptor())
        .expect("poseidon compiler catalog")
}

#[cfg(feature = "compile")]
fn compile_artifact(sdk: &Sdk, source: &str) -> tabula_sdk::Artifact {
    sdk.compile(source).expect("compile source")
}

#[cfg(not(feature = "compile"))]
fn compile_artifact(sdk: &Sdk, source: &str) -> tabula_sdk::Artifact {
    let compiled =
        compile_program_source_with_catalogs(source, &compiler_catalogs()).expect("compile source");
    register_compiled(sdk, compiled).expect("register compiled program")
}

fn open_program(sdk: &Sdk, source: &str) -> Program {
    let artifact = compile_artifact(sdk, source);
    sdk.open(artifact).expect("open artifact")
}

fn artifact_json_value(sdk: &Sdk, source: &str) -> serde_json::Value {
    serde_json::to_value(compile_artifact(sdk, source)).expect("serialize artifact")
}

fn seeded_state(program: &Program) -> State {
    program
        .state()
        .set("users", (0u64,), "tier", 0u64)
        .expect("seed tier 0")
        .set("users", (0u64,), "seen", 0u64)
        .expect("seed seen 0")
        .set("users", (1u64,), "tier", 0u64)
        .expect("seed tier 1")
        .set("users", (1u64,), "seen", 0u64)
        .expect("seed seen 1")
        .build()
}

fn context(program: &Program, caller: u64, epoch: u64) -> Context {
    program
        .context()
        .set("caller", caller)
        .expect("set caller")
        .set("epoch", epoch)
        .expect("set epoch")
        .build()
}

#[test]
fn compile_and_open_registered_program() {
    let sdk = sdk();
    let compiled = tabula_compiler::compile_program_source_with_catalogs(
        SDK_SURFACE_SOURCE,
        &compiler_catalogs(),
    )
    .expect("compile");
    let artifact = register_compiled(&sdk, compiled).expect("register compiled program");
    let reopened = sdk.open(artifact.clone()).expect("open artifact");

    assert_eq!(artifact.digest(), reopened.artifact().digest());
    assert_eq!(artifact.schema().tx_count(), reopened.schema().tx_count());
}

#[test]
fn load_artifact_accepts_fresh_registered_payload() {
    let sdk = sdk();
    let artifact = compile_artifact(&sdk, SDK_SURFACE_SOURCE);
    let bytes = serde_json::to_vec(&artifact).expect("serialize artifact");

    let loaded = sdk.load_artifact(&bytes).expect("load artifact");

    assert_eq!(artifact.digest(), loaded.digest());
    assert_eq!(artifact.schema().tx_count(), loaded.schema().tx_count());
}

#[test]
fn load_artifact_rejects_mutated_binding_program_hash() {
    let sdk = sdk();
    let mut value = artifact_json_value(&sdk, SDK_SURFACE_SOURCE);
    value["sealed"]["binding"]["program_hash"] =
        json!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");

    let err = sdk
        .load_artifact(&serde_json::to_vec(&value).expect("serialize mutated artifact payload"))
        .expect_err("mutated binding program hash must fail closed");
    assert!(
        err.to_string().contains("program binding"),
        "unexpected error: {err}"
    );
}

#[test]
fn load_artifact_rejects_mutated_binding_metadata_hash() {
    let sdk = sdk();
    let mut value = artifact_json_value(&sdk, SDK_SURFACE_SOURCE);
    value["sealed"]["binding"]["metadata_hash"] =
        json!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");

    let err = sdk
        .load_artifact(&serde_json::to_vec(&value).expect("serialize mutated artifact payload"))
        .expect_err("mutated binding metadata hash must fail closed");
    assert!(
        err.to_string().contains("program binding"),
        "unexpected error: {err}"
    );
}

#[test]
fn load_artifact_rejects_mutated_static_table_root() {
    let sdk = sdk();
    let mut value = artifact_json_value(&sdk, SDK_SURFACE_SOURCE);
    value["sealed"]["static_table_artifact"]["root"][0] = json!(19);

    let err = sdk
        .load_artifact(&serde_json::to_vec(&value).expect("serialize mutated artifact payload"))
        .expect_err("mutated static table root must fail closed");
    assert!(
        err.to_string().contains("static table artifact"),
        "unexpected error: {err}"
    );
}

#[test]
fn load_artifact_rejects_mutated_static_table_rows() {
    let sdk = sdk();
    let mut value = artifact_json_value(&sdk, SDK_SURFACE_SOURCE);
    value["sealed"]["static_table_artifact"]["rows"] = json!([{
        "relation_id": 99,
        "input_digest": [0, 0, 0, 0, 0, 0, 0, 0],
        "output_digest": [0, 0, 0, 0, 0, 0, 0, 0]
    }]);

    let err = sdk
        .load_artifact(&serde_json::to_vec(&value).expect("serialize mutated artifact payload"))
        .expect_err("mutated static table rows must fail closed");
    assert!(
        err.to_string().contains("static table artifact"),
        "unexpected error: {err}"
    );
}

#[test]
fn execute_native_batch() {
    let sdk = sdk();
    let program = open_program(&sdk, SDK_SURFACE_SOURCE);
    let snapshot = seeded_state(&program);
    let batch = program
        .batch()
        .call("register", (false, 1u64))
        .expect("register batch item")
        .call("register", (true, 0u64))
        .expect("register batch item")
        .build();

    let execution = program
        .runner()
        .execute(&snapshot, &batch, &context(&program, 7, 99))
        .expect("execute");

    assert_eq!(execution.outcomes().len(), 2);
    let post_state = execution.state_after();
    let values = state_values(&post_state);
    assert_eq!(values.get(&(0, 0, 0)), Some(&u64_portable(7)));
    assert_eq!(values.get(&(0, 0, 1)), Some(&u64_portable(1)));
    assert_eq!(values.get(&(0, 1, 0)), Some(&u64_portable(0)));
}

#[test]
fn execute_query_on_symbol_first_surface() {
    let sdk = sdk();
    let program = open_program(&sdk, SDK_SURFACE_SOURCE);
    let snapshot = seeded_state(&program);
    let runner = program.runner();

    let true_result = runner
        .query_symbol(&snapshot, "choose", (true, 5u64), &context(&program, 7, 99))
        .expect("query with true flag");
    let false_result = runner
        .query_symbol(
            &snapshot,
            "choose",
            (false, 5u64),
            &context(&program, 7, 99),
        )
        .expect("query with false flag");

    assert_eq!(true_result.decode_one::<u64>().expect("decode"), 7);
    assert_eq!(false_result.decode_one::<u64>().expect("decode"), 5);
}

#[cfg(feature = "prove")]
#[test]
fn prove_and_verify_native_execution() {
    let sdk = sdk();
    let artifact = sdk.compile(SDK_SURFACE_SOURCE).expect("compile source");
    let program = sdk.open(artifact.clone()).expect("open artifact");
    let snapshot = seeded_state(&program);
    let batch = program
        .batch()
        .call("register", (false, 1u64))
        .expect("register batch item")
        .call("register", (true, 0u64))
        .expect("register batch item")
        .build();
    let context = context(&program, 7, 99);
    let execution = program
        .runner()
        .execute(&snapshot, &batch, &context)
        .expect("execute");
    let proof = program.runner().prove(&execution).expect("prove");

    let statement = proof
        .public_statement()
        .expect("locally produced proof carries a public statement");
    assert_ne!(statement.public_context_digest.to_bytes(), [0u8; 32]);
    assert_ne!(statement.event_digest.to_bytes(), [0u8; 32]);
    assert!(proof.summary().chip_count > 0);

    program
        .verifier()
        .expect("prepare verifier")
        .verify_public_statement(&proof, statement)
        .expect("program verifier accepts proof");

    let reopened = sdk.open(artifact).expect("reopen artifact");
    reopened
        .verifier()
        .expect("prepare verifier")
        .verify_public_statement(&proof, statement)
        .expect("reopened verifier accepts proof");
}

#[cfg(feature = "prove")]
#[test]
fn proof_binary_round_trip_reuses_contract_envelope() {
    let sdk = sdk();
    let artifact = sdk.compile(SDK_SURFACE_SOURCE).expect("compile source");
    let program = sdk.open(artifact).expect("open artifact");
    let snapshot = seeded_state(&program);
    let batch = program
        .batch()
        .call("register", (true, 0u64))
        .expect("register batch item")
        .build();
    let context = context(&program, 7, 99);
    let execution = program
        .runner()
        .execute(&snapshot, &batch, &context)
        .expect("execute");
    let proof = program.runner().prove(&execution).expect("prove");

    let encoded = proof.encode_binary().expect("encode proof binary");
    let decoded = tabula_sdk::Proof::decode_binary(&encoded).expect("decode proof binary");

    // The envelope wire format does not carry the public statement; a decoded
    // proof has no associated statement and must be verified against one
    // supplied out of band.
    assert!(decoded.public_statement().is_none());
    assert_eq!(proof.binding_digest(), decoded.binding_digest());

    let statement = proof
        .public_statement()
        .expect("locally produced proof carries a public statement");
    program
        .verifier()
        .expect("prepare verifier")
        .verify_public_statement(&decoded, statement)
        .expect("verify decoded proof");
}

#[cfg(feature = "prove")]
#[test]
fn public_statement_file_round_trip_reuses_shared_contract() {
    let sdk = sdk();
    let artifact = sdk.compile(SDK_SURFACE_SOURCE).expect("compile source");
    let program = sdk.open(artifact).expect("open artifact");
    let snapshot = seeded_state(&program);
    let batch = program
        .batch()
        .call("register", (true, 0u64))
        .expect("register batch item")
        .build();
    let context = context(&program, 7, 99);
    let execution = program
        .runner()
        .execute(&snapshot, &batch, &context)
        .expect("execute");
    let proof = program.runner().prove(&execution).expect("prove");

    let statement = proof
        .public_statement()
        .expect("locally produced proof carries a public statement");
    let file = tabula_sdk::PublicStatementFile::from_public_statement(statement);
    let encoded = serde_json::to_vec_pretty(&file).expect("encode statement file");
    let decoded =
        tabula_sdk::PublicStatementFile::from_json_bytes(&encoded).expect("decode statement file");

    assert_eq!(decoded.version, tabula_sdk::PublicStatementFile::VERSION);
    assert_eq!(
        decoded
            .to_public_statement()
            .expect("reconstruct statement"),
        *statement
    );
}

#[cfg(feature = "verify")]
#[test]
fn from_envelope_rejects_unknown_proof_system_ids() {
    let envelope: tabula_sdk::interop::ProofEnvelope = serde_json::from_value(json!({
        "proof_system": 999,
        "proof_encoding": 4,
        "proof_bytes": [],
    }))
    .expect("deserialize envelope");

    let err =
        tabula_sdk::Proof::from_envelope(envelope).expect_err("unknown proof system must fail");
    assert!(
        err.to_string().contains("unsupported proof system id"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "prove")]
#[test]
fn prepare_and_reuse_runtime_and_verifier() {
    let sdk = sdk();
    let program = open_program(&sdk, SDK_SURFACE_SOURCE);
    let runner = program.runner();
    let verifier = program.verifier().expect("prepare verifier");
    let snapshot = seeded_state(&program);
    let batch = program
        .batch()
        .call("register", (false, 1u64))
        .expect("register batch item")
        .build();
    let context = context(&program, 7, 99);

    runner.warm().expect("warm runtime");
    let (_, first) = runner
        .execute_and_prove(&snapshot, &batch, &context)
        .expect("first proof");
    verifier
        .verify_public_statement(
            &first,
            first
                .public_statement()
                .expect("locally produced proof carries a public statement"),
        )
        .expect("verify first proof");

    let (_, second) = runner
        .execute_and_prove(&snapshot, &batch, &context)
        .expect("second proof");
    verifier
        .verify_public_statement(
            &second,
            second
                .public_statement()
                .expect("locally produced proof carries a public statement"),
        )
        .expect("verify second proof");

    assert_eq!(first.binding_digest(), second.binding_digest());
}

#[cfg(feature = "prove")]
#[test]
fn prove_rejects_execution_from_different_registered_program() {
    let sdk = sdk();
    let expected_artifact = sdk.compile(SDK_SURFACE_SOURCE).expect("compile source");
    let expected = sdk.open(expected_artifact).expect("open expected artifact");
    let other = open_program(&sdk, SDK_SURFACE_ALT_SCHEME_SOURCE);
    let snapshot = seeded_state(&other);
    let batch = other
        .batch()
        .call("register", (false, 1u64))
        .expect("register batch item")
        .build();
    let execution = other
        .runner()
        .execute(&snapshot, &batch, &context(&other, 7, 99))
        .expect("execute alternate program");

    let err = expected
        .runner()
        .prove(&execution)
        .expect_err("mismatched program execution should fail");
    assert!(matches!(
        err,
        tabula_sdk::SdkError::ExecutionProgramMismatch
    ));
}
