#![cfg(feature = "prove")]
#![allow(missing_docs)]

use tabula_ir as ir;
use tabula_testing::exec::{context_input, register_program_from_source, state_snapshot, tx_batch};
use tabula_testing::runtime::build_runtime;
use tabula_types::{bool_portable, u64_portable, u64_typed};

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

fn context(caller: u64, epoch: u64) -> ir::ContextInput {
    context_input([
        (ir::ContextFieldId(0), u64_portable(caller)),
        (ir::ContextFieldId(1), u64_portable(epoch)),
    ])
}

fn seeded_snapshot(
    registered: &tabula_compiler::RegisteredProgram,
) -> tabula_runtime::StateSnapshot {
    state_snapshot(
        registered,
        &[
            (
                tabula_ir::TableId(0),
                tabula_core::RowKey(0),
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                tabula_core::RowKey(0),
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                tabula_core::RowKey(1),
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
            (
                tabula_ir::TableId(0),
                tabula_core::RowKey(1),
                tabula_ir::FieldId(1),
                u64_portable(0),
            ),
        ],
    )
}

fn register_entry_id(runtime: &tabula_runtime::TabulaRuntime) -> tabula_ir::EntryId {
    runtime
        .execution_program()
        .program()
        .entries
        .iter()
        .find(|entry| entry.symbol == "register")
        .map(|entry| entry.id)
        .expect("register entry")
}

fn choose_entry_id(runtime: &tabula_runtime::TabulaRuntime) -> tabula_ir::EntryId {
    runtime
        .execution_program()
        .program()
        .entries
        .iter()
        .find(|entry| entry.symbol == "choose")
        .map(|entry| entry.id)
        .expect("choose entry")
}

#[test]
fn query_executes_but_remains_execution_only() {
    let registered = register_program_from_source(proving_source());
    let runtime = build_runtime(registered);
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
fn tx_batch_proves_and_verifies_mixed_surface() {
    let registered = register_program_from_source(proving_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_runtime(registered);
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

    let verified = runtime
        .prove_and_verify(&tabula_runtime::ProveInput {
            snapshot: &snapshot,
            batch: &txs,
            context: &context,
            executed: &executed,
        })
        .expect("prove and verify");

    assert!(verified.verified);
    assert_eq!(verified.statement.public.public_context.len(), 2);
    assert_ne!(verified.statement.public.event_digest, [0u8; 32]);
    assert_eq!(
        verified.statement.old_state_root,
        verified.proof.statement.old_root.to_bytes()
    );
    assert_eq!(
        verified.statement.new_state_root,
        verified.proof.statement.new_root.to_bytes()
    );
    assert_eq!(
        verified.proof.statement_digest,
        verified
            .statement
            .statement_hash_bytes()
            .expect("statement hash"),
    );
}

#[test]
fn statement_hash_changes_with_batch_context_and_binding() {
    let registered = register_program_from_source(proving_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_runtime(registered);
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

    let prove_a = runtime
        .execute_and_prove(&snapshot, &txs_a, &context_a)
        .expect("prove a");
    let prove_b = runtime
        .execute_and_prove(&snapshot, &txs_b, &context_a)
        .expect("prove b");
    let prove_c = runtime
        .execute_and_prove(&snapshot, &txs_a, &context_b)
        .expect("prove c");

    assert_ne!(
        prove_a
            .statement
            .statement_hash_bytes()
            .expect("statement hash a"),
        prove_b
            .statement
            .statement_hash_bytes()
            .expect("statement hash b"),
    );
    assert_ne!(
        prove_a.statement.public.event_digest,
        prove_b.statement.public.event_digest
    );
    assert_ne!(
        prove_a
            .statement
            .statement_hash_bytes()
            .expect("statement hash a"),
        prove_c
            .statement
            .statement_hash_bytes()
            .expect("statement hash c"),
    );
    assert_ne!(
        prove_a.statement.public.public_context,
        prove_c.statement.public.public_context,
    );

    let alt_registered = register_program_from_source(proving_source_alt_scheme());
    let alt_binding = alt_registered.binding().clone();
    let alt_snapshot = seeded_snapshot(&alt_registered);
    let alt_runtime = build_runtime(alt_registered);
    let alt_register = register_entry_id(&alt_runtime);
    let alt_txs = tx_batch(vec![ir::EntryCall {
        entry_id: alt_register,
        params: vec![bool_portable(false), u64_portable(1)],
    }]);
    let alt_proved = alt_runtime
        .execute_and_prove(&alt_snapshot, &alt_txs, &context_a)
        .expect("prove alt");

    assert_ne!(prove_a.statement.binding, alt_binding);
    assert_ne!(
        prove_a
            .statement
            .statement_hash_bytes()
            .expect("statement hash a"),
        alt_proved
            .statement
            .statement_hash_bytes()
            .expect("statement hash alt"),
    );
}
