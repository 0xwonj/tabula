#![cfg(feature = "prove")]
#![allow(missing_docs)]

use tabula_ir as ir;
use tabula_testing::exec::{
    context_input, logical_state_snapshot, register_program_from_source, tx_batch,
};
use tabula_testing::runtime::{build_executor, build_prover, build_verifier_from_registered};
use tabula_types::{bool_portable, u64_portable, u64_typed};

fn relation_source() -> &'static str {
    r#"
program RelationProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };
relation ValidEpoch(epoch: u64) = range(10, 13);
relation PreferredCaller(actor: u64) = set { 7, 8 };
relation PromoteTier(tier: u64) -> promoted: u64 = map {
  0 => 1,
  1 => 2,
  2 => 3,
  3 => 3,
};

query quote(tier: u64) -> u64 {
  assert relation ValidEpoch(epoch);
  return eval relation PromoteTier(tier);
}

tx enroll(flag: bool, id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  assert relation ValidEpoch(epoch);
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert relation PreferredCaller(caller);
  }
  match id {
    0 => {
      assert relation PreferredCaller(caller);
    }
    _ => {
      assert relation AllowedTier(tier);
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
                vec![u64_portable(1)],
                tabula_ir::FieldId(0),
                u64_portable(0),
            ),
        ],
    )
}

fn entry_id(executor: &tabula_runtime::PreparedExecutor, symbol: &str) -> tabula_ir::EntryId {
    executor
        .entry_id_by_symbol(symbol)
        .unwrap_or_else(|| panic!("missing entry '{symbol}'"))
}

#[test]
fn query_executes_relations_but_remains_execution_only() {
    let registered = register_program_from_source(relation_source());
    let runtime = build_executor(registered);
    let snapshot = runtime.empty_state_snapshot();
    let query = entry_id(&runtime, "quote");

    let result = runtime
        .execute_query(&snapshot, query, &[u64_portable(2)], &context(7, 11))
        .expect("query with relation eval");

    assert_eq!(result.returns, vec![u64_typed(3)]);
    assert!(result.state_effects.is_empty());
    assert!(result.event_effects.is_empty());
    assert_eq!(result.relation_effects.len(), 2);
}

#[test]
fn tx_batch_proves_and_verifies_static_relations_with_control() {
    let registered = register_program_from_source(relation_source());
    let snapshot = seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let enroll = entry_id(&runtime, "enroll");
    let batch = tx_batch(vec![
        ir::EntryCall {
            entry_id: enroll,
            params: vec![bool_portable(false), u64_portable(1), u64_portable(2)],
        },
        ir::EntryCall {
            entry_id: enroll,
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        },
    ]);
    let ctx = context(7, 11);

    let executed = runtime
        .execute_batch(&snapshot, &batch, &ctx)
        .expect("execute relation batch");
    let txs = executed.successful_txs().collect::<Vec<_>>();
    assert_eq!(txs.len(), 2);
    assert_eq!(txs[0].relation_effects.len(), 4);
    assert!(txs[0].state_effects.is_empty());
    assert_eq!(txs[1].relation_effects.len(), 4);
    assert_eq!(txs[1].state_effects.len(), 1);

    let verified = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput::new(&snapshot, &batch, &ctx, &executed),
        )
        .expect("prove and verify relation batch");

    assert!(verified.verified());
    assert_ne!(
        verified.public_statement().public_context_digest.to_bytes(),
        [0u8; 32]
    );
    assert_ne!(
        verified.public_statement().event_digest.to_bytes(),
        [0u8; 32]
    );
}

#[test]
fn range_and_set_relations_normalize_and_prove() {
    let registered = register_program_from_source(relation_source());
    let manifest = &registered.program().relation_manifest.entries;

    let valid_epoch = manifest
        .iter()
        .find(|entry| entry.descriptor.symbol == "ValidEpoch")
        .expect("ValidEpoch relation");
    assert!(matches!(
        valid_epoch.binding,
        ir::RelationBinding::EnumSet { .. }
    ));

    let preferred_caller = manifest
        .iter()
        .find(|entry| entry.descriptor.symbol == "PreferredCaller")
        .expect("PreferredCaller relation");
    assert!(matches!(
        preferred_caller.binding,
        ir::RelationBinding::Map { .. }
    ));

    let snapshot = seeded_snapshot(&registered);
    let runtime = build_executor(registered.clone());
    let prover = build_prover(registered.clone());
    let verifier = build_verifier_from_registered(&registered);
    let enroll = entry_id(&runtime, "enroll");
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: enroll,
        params: vec![bool_portable(false), u64_portable(0), u64_portable(1)],
    }]);
    let ctx = context(7, 12);

    let executed = runtime
        .execute_batch(&snapshot, &batch, &ctx)
        .expect("execute normalized relations");
    let proved = prover
        .prove_and_verify(
            &verifier,
            &tabula_runtime::ProveInput::new(&snapshot, &batch, &ctx, &executed),
        )
        .expect("prove normalized relations");

    assert_ne!(proved.public_statement().event_digest.to_bytes(), [0u8; 32]);
    assert_ne!(proved.public_statement().old_root.to_bytes(), [0u8; 32]);
    assert_ne!(proved.public_statement().new_root.to_bytes(), [0u8; 32]);
}
