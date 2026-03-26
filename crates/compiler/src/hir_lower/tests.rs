use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_runtime::semantics::RuntimeProgram;
use tabula_types::{TypeRuntimeRegistry, u64_typed};

use super::lower_hir_to_mir;
use crate::mir;

fn prelude() -> tabula_lang::FrontendPrelude {
    tabula_lang::FrontendPrelude::new(
        tabula_profile::builtin_semantic_registry().expect("registry"),
        vec![tabula_lang::CapabilityPreludeEntry {
            path: "poseidon_hash".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_BYTES32_ID],
            totality: hir::CapabilityTotality::Total,
            query_policy: hir::CapabilityQueryPolicy::QuerySafe,
            proof_visibility: hir::CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: Some(hir::HashFamily::Poseidon),
        }],
    )
    .expect("prelude")
}

#[test]
fn lowering_builds_verified_mir_from_hir() {
    let source = r#"
use capability poseidon_hash;

program Registry

state {
  table users(key id: u64) {
    active: bool @ssmc;
    tier: u64 @ssmc;
  }
}

const MAX_TIER: u64 = 3;

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };

fn validate_tier(tier: u64) {
  assert relation AllowedTier(tier);
  return;
}

tx register(id: u64, tier: u64) {
  validate_tier(tier);
  let digest = poseidon_hash(tier);
  assert select(true, true, true);
  users[id].active = true;
  users[id].tier = tier;
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(99)).expect("mir");
    assert_eq!(mir.program_id, ir::ProgramId(99));
    let verified = mir::verify_program(mir).expect("verified");
    let analyzed = mir::analyze_program(verified).expect("analyzed");
    let normalized = mir::inline_functions(&analyzed).expect("normalized");
    let canonicalized = mir::canonicalize_program(&normalized).expect("canonicalized");
    let analyzed = mir::analyze_program(canonicalized).expect("reanalyzed");
    let canonical = mir::lower_to_canonical(&analyzed).expect("canonical");
    let validated = ir::ValidatedProgram::try_from(canonical).expect("validated");
    let runtime = RuntimeProgram::from_validated_program(validated).expect("runtime");
    let state = InMemoryState::default();
    let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
    let exec_ctx = exec::ExecContext {
        hasher: &Blake3Hasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: None,
    };
    let context = exec::ContextValues::default();
    let result = exec::execute_batch(
        runtime.execution(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2)],
        }],
        &context,
        &state,
        &exec_ctx,
    )
    .expect("execute");
    assert_eq!(result.txs.len(), 1);
    assert!(matches!(
        result.txs[0],
        exec::TxExecutionOutcome::Success(_)
    ));
}

#[test]
fn lowering_supports_v2_context_query_and_emit() {
    let source = r#"
program Registry

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

query current_actor(seed: u64) -> u64 {
  let actor = caller;
  return select(true, actor, seed);
}

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(7)).expect("mir");
    let query = mir
        .callables
        .iter()
        .find(|callable| callable.kind == mir::CallableKind::Query)
        .expect("query callable");
    assert!(
        query
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::BindValue { .. }))
    );

    let tx = mir
        .callables
        .iter()
        .find(|callable| callable.kind == mir::CallableKind::Tx)
        .expect("tx callable");
    assert!(
        tx.body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::EmitEvent { .. }))
    );
}

#[test]
fn lowering_supports_v3_statement_level_if_and_match() {
    let source = r#"
program Control

tx choose(flag: bool, value: u64) {
  if flag {
    let selected = value;
  } else {
    let selected = 0;
  }
  match value {
    0 => {
      assert true;
    }
    _ => {
      assert true;
    }
  }
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(1)).expect("mir");
    let callable = mir
        .callables
        .iter()
        .find(|callable| callable.symbol == "choose")
        .unwrap();
    assert!(
        callable
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::If { .. }))
    );
    assert!(
        callable
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::Match { .. }))
    );
}
