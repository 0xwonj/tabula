//! Smoke tests for [`tabula_runtime::PreparedExecutor`].
//!
//! Parallel to the `PreparedProver` determinism guardrail: these exercise
//! executor-side determinism (execute_batch twice → identical journal)
//! and a query return-value spot-check.

#![cfg(feature = "verify")]

use std::sync::Arc;

use tabula_ir as ir;
use tabula_runtime::{PreparedExecutor, PreparedOptions, prepare_executor};
use tabula_testing::exec::{context_input, register_program_from_source, tx_batch};
use tabula_types::u64_portable;

fn sample_source() -> &'static str {
    r#"
program ExecutorSmoke

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2 };

tx enroll(id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  accounts[id].tier = tier;
  return;
}

query tier_of(id: u64) -> u64 {
  return accounts[id].tier;
}
"#
}

fn build_executor() -> PreparedExecutor {
    let registered = register_program_from_source(sample_source());
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    prepare_executor(Arc::new(registered), &opts).expect("prepare executor")
}

fn entry_id_by_symbol(executor: &PreparedExecutor, symbol: &str) -> ir::EntryId {
    executor
        .entry_id_by_symbol(symbol)
        .unwrap_or_else(|| panic!("entry '{symbol}' missing from compiled program"))
}

#[test]
fn execute_batch_twice_is_deterministic() {
    let executor = build_executor();
    let snapshot = executor
        .materialize_logical_state([
            (
                ir::TableId(0),
                vec![u64_portable(0)],
                ir::FieldId(0),
                u64_portable(0),
            ),
            (
                ir::TableId(0),
                vec![u64_portable(1)],
                ir::FieldId(0),
                u64_portable(0),
            ),
        ])
        .expect("materialize initial snapshot");

    let entry_id = entry_id_by_symbol(&executor, "enroll");
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id,
        params: vec![u64_portable(0), u64_portable(1)],
    }]);
    let ctx = context_input([(ir::ContextFieldId(0), u64_portable(7))]);

    let journal1 = executor
        .execute_batch(&snapshot, &batch, &ctx)
        .expect("first execute_batch");
    let journal2 = executor
        .execute_batch(&snapshot, &batch, &ctx)
        .expect("second execute_batch");

    // `ExecutionJournal` is not PartialEq directly; compare a
    // canonical projection. The successful-tx count plus the write-set
    // final state captures the observable behavior we care about.
    assert_eq!(
        journal1.successful_txs().count(),
        journal2.successful_txs().count(),
        "successful tx counts must match across runs"
    );
    assert_eq!(
        journal1.state_summary.write_set_final.len(),
        journal2.state_summary.write_set_final.len(),
        "write-set final lengths must match across runs"
    );
    assert_eq!(
        journal1
            .state_summary
            .write_set_final
            .iter()
            .map(|w| (w.key.clone(), w.value.clone()))
            .collect::<Vec<_>>(),
        journal2
            .state_summary
            .write_set_final
            .iter()
            .map(|w| (w.key.clone(), w.value.clone()))
            .collect::<Vec<_>>(),
        "write-set final contents must match across runs"
    );
}

#[test]
fn execute_query_returns_expected_value() {
    let executor = build_executor();
    // Pre-populate the account with tier = 2 so the query has something
    // to return.
    let snapshot = executor
        .materialize_logical_state([(
            ir::TableId(0),
            vec![u64_portable(42)],
            ir::FieldId(0),
            u64_portable(2),
        )])
        .expect("materialize snapshot with tier=2");

    let entry_id = entry_id_by_symbol(&executor, "tier_of");
    let ctx = context_input([(ir::ContextFieldId(0), u64_portable(7))]);
    let result = executor
        .execute_query(&snapshot, entry_id, &[u64_portable(42)], &ctx)
        .expect("execute_query");

    assert_eq!(result.returns.len(), 1, "tier_of must return exactly one value");
    assert_eq!(
        result.returns[0].payload(),
        &2u64.to_le_bytes()[..],
        "tier_of(42) must return the stored tier (=2)"
    );
}
