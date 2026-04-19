#![cfg(feature = "prove")]
#![allow(missing_docs)]

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_chips::event_transcript::EVENT_TRANSCRIPT_CHIP_ID;
use tabula_chips::public_context_transcript::PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID;
use tabula_ir as ir;
use tabula_machine::VerificationError;
use tabula_testing::exec::{
    context_input, logical_state_snapshot, register_program_from_source, tx_batch,
};
use tabula_testing::runtime::{build_executor, build_prover, build_verifier_from_registered};
use tabula_types::{bool_portable, i64_portable, u64_portable, u64_typed};

fn proving_source() -> &'static str {
    r#"
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
"#
}

fn proving_source_alt_scheme() -> &'static str {
    r#"
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
"#
}

fn bool_key_proving_source() -> &'static str {
    r#"
program BoolKeyProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table flags(key active: bool) {
    tier: u64 @ssmc;
    seen: u64 @ssmc;
  }
}

tx update(active: bool) {
  if active {
    flags[active].tier = caller;
    flags[active].seen = epoch;
  } else {
    flags[active].tier = epoch;
    flags[active].seen = caller;
  }
  return;
}
"#
}

fn i64_key_proving_source() -> &'static str {
    r#"
program I64KeyProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table deltas(key offset: i64) {
    tier: u64 @ssmc;
    seen: u64 @ssmc;
  }
}

tx update(offset: i64) {
  deltas[offset].tier = caller;
  deltas[offset].seen = epoch;
  return;
}
"#
}

fn context(caller: u64, epoch: u64) -> ir::ContextInput {
    context_input([
        (ir::ContextFieldId(0), u64_portable(caller)),
        (ir::ContextFieldId(1), u64_portable(epoch)),
    ])
}

fn seeded_snapshot(
    registered: &tabula_compiler::RegisteredProgram,
) -> tabula_runtime::CommittedStateSnapshot {
    logical_state_snapshot(
        registered,
        &[
            (
                tabula_ir::TableId(0),
                vec![u64_portable(0)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![u64_portable(0)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![u64_portable(1)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![u64_portable(1)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
        ],
    )
}

fn bool_key_seeded_snapshot(
    registered: &tabula_compiler::RegisteredProgram,
) -> tabula_runtime::CommittedStateSnapshot {
    logical_state_snapshot(
        registered,
        &[
            (
                tabula_ir::TableId(0),
                vec![bool_portable(false)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![bool_portable(false)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![bool_portable(true)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![bool_portable(true)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
        ],
    )
}

fn i64_key_seeded_snapshot(
    registered: &tabula_compiler::RegisteredProgram,
) -> tabula_runtime::CommittedStateSnapshot {
    logical_state_snapshot(
        registered,
        &[
            (
                tabula_ir::TableId(0),
                vec![i64_portable(-1)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![i64_portable(-1)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![i64_portable(1)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                vec![i64_portable(1)],
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
        ],
    )
}

fn register_entry_id(executor: &tabula_runtime::PreparedExecutor) -> tabula_ir::EntryId {
    executor
        .entry_id_by_symbol("register")
        .expect("register entry")
}

fn choose_entry_id(executor: &tabula_runtime::PreparedExecutor) -> tabula_ir::EntryId {
    executor.entry_id_by_symbol("choose").expect("choose entry")
}

fn update_entry_id(executor: &tabula_runtime::PreparedExecutor) -> tabula_ir::EntryId {
    executor.entry_id_by_symbol("update").expect("update entry")
}

fn prove_native_batch() -> (
    tabula_runtime::PreparedVerifier,
    tabula_runtime::ProofOutcome,
) {
    let registered = register_program_from_source(proving_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let register = register_entry_id(&runtime);
    let batch = tx_batch(vec![
        ir::EntryCall {
            entry_id: register,
            params: vec![bool_portable(false), u64_portable(1)],
        },
        ir::EntryCall {
            entry_id: register,
            params: vec![bool_portable(true), u64_portable(0)],
        },
    ]);
    let context = context(7, 99);
    let executed = runtime
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute native batch");
    let proved = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput { snapshot: &snapshot, batch: &batch, context: &context, executed: &executed },
        )
        .expect("prove native batch");
    (verifier, proved)
}

#[test]
fn query_executes_but_remains_execution_only() {
    let registered = register_program_from_source(proving_source());
    let runtime = build_executor(registered);
    let snapshot = runtime.empty_state_snapshot();
    let choose = choose_entry_id(&runtime);

    let true_result = runtime
        .execute_query(
            &snapshot,
            choose,
            &[bool_portable(true), u64_portable(5)],
            &context(7, 99),
        )
        .expect("query true");
    let false_result = runtime
        .execute_query(
            &snapshot,
            choose,
            &[bool_portable(false), u64_portable(5)],
            &context(7, 99),
        )
        .expect("query false");

    assert_eq!(true_result.returns, vec![u64_typed(7)]);
    assert_eq!(false_result.returns, vec![u64_typed(5)]);
    assert!(true_result.state_effects.is_empty());
    assert!(false_result.event_effects.is_empty());
}

#[test]
fn unary_bool_key_batch_executes_projects_and_proves() {
    let registered = register_program_from_source(bool_key_proving_source());
    let snapshot = bool_key_seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let update = update_entry_id(&runtime);
    let batch = tx_batch(vec![
        ir::EntryCall {
            entry_id: update,
            params: vec![bool_portable(false)],
        },
        ir::EntryCall {
            entry_id: update,
            params: vec![bool_portable(true)],
        },
    ]);
    let ctx = context(7, 99);

    let receipt = runtime
        .execute_batch_receipt(&snapshot, &batch, &ctx)
        .expect("execute bool-key batch");
    let projected = runtime
        .project_logical_state(receipt.state_after())
        .expect("project logical bool-key state");

    for expected in [
        (
            tabula_ir::TableId(0),
            vec![bool_portable(false)],
            tabula_ir::FieldId(0),
            u64_portable(99),
        ),
        (
            tabula_ir::TableId(0),
            vec![bool_portable(false)],
            tabula_ir::FieldId(1),
            u64_portable(7),
        ),
        (
            tabula_ir::TableId(0),
            vec![bool_portable(true)],
            tabula_ir::FieldId(0),
            u64_portable(7),
        ),
        (
            tabula_ir::TableId(0),
            vec![bool_portable(true)],
            tabula_ir::FieldId(1),
            u64_portable(99),
        ),
    ] {
        assert!(
            projected.contains(&expected),
            "missing projected logical state cell: {expected:?}; projected={projected:?}"
        );
    }

    let verified = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &ctx,
                executed: receipt.journal(),
            },
        )
        .expect("prove and verify bool-key batch");

    assert!(verified.verified());
    assert_ne!(verified.public_statement().old_root.to_bytes(), [0u8; 32]);
    assert_ne!(verified.public_statement().new_root.to_bytes(), [0u8; 32]);
}

#[test]
fn unary_i64_key_batch_executes_projects_and_proves() {
    let registered = register_program_from_source(i64_key_proving_source());
    let snapshot = i64_key_seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let update = update_entry_id(&runtime);
    let batch = tx_batch(vec![
        ir::EntryCall {
            entry_id: update,
            params: vec![i64_portable(-1)],
        },
        ir::EntryCall {
            entry_id: update,
            params: vec![i64_portable(1)],
        },
    ]);
    let ctx = context(7, 99);

    let receipt = runtime
        .execute_batch_receipt(&snapshot, &batch, &ctx)
        .expect("execute i64-key batch");
    let projected = runtime
        .project_logical_state(receipt.state_after())
        .expect("project logical i64-key state");

    for expected in [
        (
            tabula_ir::TableId(0),
            vec![i64_portable(-1)],
            tabula_ir::FieldId(0),
            u64_portable(7),
        ),
        (
            tabula_ir::TableId(0),
            vec![i64_portable(-1)],
            tabula_ir::FieldId(1),
            u64_portable(99),
        ),
        (
            tabula_ir::TableId(0),
            vec![i64_portable(1)],
            tabula_ir::FieldId(0),
            u64_portable(7),
        ),
        (
            tabula_ir::TableId(0),
            vec![i64_portable(1)],
            tabula_ir::FieldId(1),
            u64_portable(99),
        ),
    ] {
        assert!(
            projected.contains(&expected),
            "missing projected logical state cell: {expected:?}; projected={projected:?}"
        );
    }

    let verified = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &ctx,
                executed: receipt.journal(),
            },
        )
        .expect("prove and verify i64-key batch");

    assert!(verified.verified());
    assert_ne!(verified.public_statement().old_root.to_bytes(), [0u8; 32]);
    assert_ne!(verified.public_statement().new_root.to_bytes(), [0u8; 32]);
}

#[test]
fn tx_batch_proves_and_verifies_mixed_surface() {
    let registered = register_program_from_source(proving_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let register = register_entry_id(&runtime);
    let txs = tx_batch(vec![
        ir::EntryCall {
            entry_id: register,
            params: vec![bool_portable(false), u64_portable(1)],
        },
        ir::EntryCall {
            entry_id: register,
            params: vec![bool_portable(true), u64_portable(0)],
        },
    ]);
    let context = context(7, 99);

    let executed = runtime
        .execute_batch(&snapshot, &txs, &context)
        .expect("execute batch");
    let txs_out = executed.successful_txs().collect::<Vec<_>>();
    assert_eq!(txs_out.len(), 2);
    assert_eq!(txs_out[0].event_effects.len(), 1);
    assert!(txs_out[0].state_effects.is_empty());
    assert!(txs_out[1].event_effects.is_empty());
    assert!(!txs_out[1].state_effects.is_empty());

    let verified = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput { snapshot: &snapshot, batch: &txs, context: &context, executed: &executed },
        )
        .expect("prove and verify");

    assert!(verified.verified());
    assert_ne!(
        verified.public_statement().public_context_digest.to_bytes(),
        [0u8; 32]
    );
    assert_ne!(
        verified.public_statement().event_digest.to_bytes(),
        [0u8; 32]
    );
    assert_ne!(verified.proof().binding_digest, [0u8; 32]);
}

#[test]
fn binding_digest_changes_with_batch_context_and_binding() {
    let registered = register_program_from_source(proving_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let register = register_entry_id(&runtime);
    let txs_a = tx_batch(vec![ir::EntryCall {
        entry_id: register,
        params: vec![bool_portable(false), u64_portable(1)],
    }]);
    let txs_b = tx_batch(vec![ir::EntryCall {
        entry_id: register,
        params: vec![bool_portable(false), u64_portable(2)],
    }]);
    let context_a = context(7, 99);
    let context_b = context(8, 99);

    let exec_a = runtime
        .execute_batch(&snapshot, &txs_a, &context_a)
        .expect("exec a");
    let prove_a = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput { snapshot: &snapshot, batch: &txs_a, context: &context_a, executed: &exec_a },
        )
        .expect("prove a");
    let exec_b = runtime
        .execute_batch(&snapshot, &txs_b, &context_a)
        .expect("exec b");
    let prove_b = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput { snapshot: &snapshot, batch: &txs_b, context: &context_a, executed: &exec_b },
        )
        .expect("prove b");
    let exec_c = runtime
        .execute_batch(&snapshot, &txs_a, &context_b)
        .expect("exec c");
    let prove_c = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput { snapshot: &snapshot, batch: &txs_a, context: &context_b, executed: &exec_c },
        )
        .expect("prove c");

    assert_ne!(
        prove_a.proof().binding_digest,
        prove_b.proof().binding_digest
    );
    assert_ne!(
        prove_a.public_statement().event_digest,
        prove_b.public_statement().event_digest
    );
    assert_ne!(
        prove_a.proof().binding_digest,
        prove_c.proof().binding_digest
    );
    assert_ne!(
        prove_a.public_statement().public_context_digest,
        prove_c.public_statement().public_context_digest
    );

    let alt_registered = register_program_from_source(proving_source_alt_scheme());
    let alt_snapshot = seeded_snapshot(&alt_registered);
    let alt_runtime = build_executor(alt_registered.clone());
    let alt_prover = build_prover(alt_registered.clone());
    let alt_verifier = build_verifier_from_registered(&alt_registered);
    let alt_register = register_entry_id(&alt_runtime);
    let alt_txs = tx_batch(vec![ir::EntryCall {
        entry_id: alt_register,
        params: vec![bool_portable(false), u64_portable(1)],
    }]);
    let alt_exec = alt_runtime
        .execute_batch(&alt_snapshot, &alt_txs, &context_a)
        .expect("exec alt");
    let alt_proved = alt_prover
        .prove_and_verify(
            &alt_verifier,
            &tabula_runtime::ProveInput { snapshot: &alt_snapshot, batch: &alt_txs, context: &context_a, executed: &alt_exec },
        )
        .expect("prove alt");

    assert_ne!(
        prove_a.proof().binding_digest,
        alt_proved.proof().binding_digest
    );
}

#[test]
fn verifier_rejects_missing_column_proof_manifest_entry() {
    let (verifier, proved) = prove_native_batch();
    let (mut proof, _, public_statement, _, _) = proved.into_parts();
    proof.columns.pop();

    let err = verifier
        .verify(&proof, &public_statement)
        .expect_err("missing column proof must fail verification");
    assert!(matches!(
        err,
        tabula_runtime::VerifyError::Verification(VerificationError::ColumnProofCountMismatch {
            expected: 2,
            got: 1
        })
    ));
}

#[test]
fn verifier_rejects_permuted_column_proof_manifest_order() {
    let (verifier, proved) = prove_native_batch();
    let (mut proof, _, public_statement, _, _) = proved.into_parts();
    proof.columns.swap(0, 1);

    let err = verifier
        .verify(&proof, &public_statement)
        .expect_err("permuted column proof order must fail verification");
    assert!(matches!(
        err,
        tabula_runtime::VerifyError::Verification(VerificationError::ColumnOrderMismatch {
            index: 0,
            ..
        })
    ));
}

#[test]
fn verifier_rejects_duplicate_column_proof_manifest_entry() {
    let (verifier, proved) = prove_native_batch();
    let (mut proof, _, public_statement, _, _) = proved.into_parts();
    proof.columns[1].key = proof.columns[0].key;

    let err = verifier
        .verify(&proof, &public_statement)
        .expect_err("duplicate column proof manifest entry must fail verification");
    assert!(matches!(
        err,
        tabula_runtime::VerifyError::Verification(VerificationError::ColumnOrderMismatch {
            index: 1,
            ..
        })
    ));
}

#[test]
fn verifier_rejects_wrong_public_context_digest() {
    let (verifier, proved) = prove_native_batch();
    let mut wrong_statement = proved.public_statement().clone();
    wrong_statement.public_context_digest.0[0] += KoalaBear::ONE;

    let err = verifier
        .verify(proved.proof(), &wrong_statement)
        .expect_err("wrong public-context digest must fail verification");
    assert!(
        err.to_string()
            .contains("proof binding digest does not match the artifact-bound public statement"),
        "unexpected error: {err}"
    );
}

#[test]
fn verifier_rejects_wrong_applied_tx_digest() {
    let (verifier, proved) = prove_native_batch();
    let mut wrong_statement = proved.public_statement().clone();
    wrong_statement.applied_tx_digest.0[0] += KoalaBear::ONE;

    let err = verifier
        .verify(proved.proof(), &wrong_statement)
        .expect_err("wrong applied tx digest must fail verification");
    assert!(
        err.to_string()
            .contains("proof binding digest does not match the artifact-bound public statement"),
        "unexpected error: {err}"
    );
}

#[test]
fn verifier_rejects_wrong_event_digest() {
    let (verifier, proved) = prove_native_batch();
    let mut wrong_statement = proved.public_statement().clone();
    wrong_statement.event_digest.0[0] += KoalaBear::ONE;

    let err = verifier
        .verify(proved.proof(), &wrong_statement)
        .expect_err("wrong event digest must fail verification");
    assert!(
        err.to_string()
            .contains("proof binding digest does not match the artifact-bound public statement"),
        "unexpected error: {err}"
    );
}

#[test]
fn verifier_rejects_mutated_public_context_chip_digest() {
    let (verifier, proved) = prove_native_batch();
    let (mut proof, _, public_statement, _, _) = proved.into_parts();
    let opening = proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID)
        .expect("public-context transcript chip opening");
    opening.public_values[0] += KoalaBear::ONE;

    let err = verifier
        .verify(&proof, &public_statement)
        .expect_err("mutated public-context chip digest must fail verification");
    assert!(
        err.to_string().contains(
            "public-context transcript chip digest does not match the proved public statement"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn verifier_rejects_mutated_event_chip_digest() {
    let (verifier, proved) = prove_native_batch();
    let (mut proof, _, public_statement, _, _) = proved.into_parts();
    let opening = proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == EVENT_TRANSCRIPT_CHIP_ID)
        .expect("event transcript chip opening");
    opening.public_values[0] += KoalaBear::ONE;

    let err = verifier
        .verify(&proof, &public_statement)
        .expect_err("mutated event chip digest must fail verification");
    assert!(
        err.to_string()
            .contains("event transcript chip digest does not match the proved public statement"),
        "unexpected error: {err}"
    );
}
