//! Integration tests for the relation-proof path.
//!
//! Included into [`crate::prover`] via `#[path]` so tests retain
//! `pub(crate)` access to `PreparedProver` fields without adding
//! accessors that are not needed outside of tests.
//!
//! Split across three sub-modules by concern:
//! - [`witness_labels_tests`]: witness-label contract, chip public values,
//!   event transcript witness.
//! - [`relation_trace_tests`]: relation-table chip trace generation, prover
//!   integration, snapshot / execution infrastructure.
//! - [`tampering_tests`]: Class-B tamper tests that construct chip-row values
//!   to exercise the prover's rejection semantics.
//!
//! Sub-files end in `_tests.rs` so the F0 chip-row boundary guardrail
//! (`crates/runtime/tests/no_chip_rows_in_runtime.rs`) continues to skip
//! them. No change to that guardrail was needed.

use super::*;
use crate::{PreparedExecutor, prepare_executor};

use std::sync::Arc;

use tabula_testing::exec::{context_input, register_program_from_source, tx_batch};
use tabula_types::{bool_portable, u64_portable};

/// Source program for relation-proof tests.
pub fn relation_source() -> &'static str {
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

tx enroll(flag: bool, id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  assert relation ValidEpoch(epoch);
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert relation PreferredCaller(caller);
  }
  return;
}
"#
}

/// Source program for event-transcript tests.
pub fn event_debug_source() -> &'static str {
    r#"
program EventTranscriptDebug

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#
}

/// Source program for guarded-relation tests.
pub fn guarded_relation_source() -> &'static str {
    r#"
program GuardedRelation

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation PromoteTier(tier: u64) -> promoted: u64 = map {
  1 => 2,
  2 => 3,
  3 => 3,
};

tx maybe_promote(flag: bool, id: u64, tier: u64) {
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert true;
  }
  return;
}
"#
}

/// Source program for capability-rejection tests.
pub fn capability_source() -> &'static str {
    r#"
use capability demo_hash;

program DeferredCapability

tx scan(id: u64) {
  let digest = demo_hash(id);
  assert true;
  return;
}
"#
}

/// Build a `ContextInput` with `caller` and `epoch` fields.
pub fn relation_context(caller: u64, epoch: u64) -> ir::ContextInput {
    context_input([
        (ir::ContextFieldId(0), u64_portable(caller)),
        (ir::ContextFieldId(1), u64_portable(epoch)),
    ])
}

/// Build a `ContextInput` with only a `caller` field.
pub fn guarded_context(caller: u64) -> ir::ContextInput {
    context_input([(ir::ContextFieldId(0), u64_portable(caller))])
}

/// Materialize a committed state snapshot with two pre-seeded account rows.
pub fn relation_snapshot(registered: &RegisteredProgram) -> CommittedStateSnapshot {
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    executor
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
        .expect("build relation snapshot")
}

/// Build a registered program, executor, and prover from a source string.
pub fn executor_and_prover_for_source(
    source: &str,
) -> (RegisteredProgram, PreparedExecutor, crate::PreparedProver) {
    let registered = register_program_from_source(source);
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover =
        crate::prepare_prover(Arc::new(registered.clone()), &opts).expect("build prepared prover");
    (registered, executor, prover)
}

/// Look up an entry ID by symbol, panicking if missing.
pub fn entry_id_for(executor: &PreparedExecutor, symbol: &str) -> ir::EntryId {
    executor
        .entry_id_by_symbol(symbol)
        .unwrap_or_else(|| panic!("missing entry '{symbol}'"))
}

/// Assemble a `ProveInput` from its four components.
pub fn prove_input<'a>(
    snapshot: &'a CommittedStateSnapshot,
    batch: &'a ir::EntryBatch,
    context: &'a ir::ContextInput,
    executed: &'a exec::ExecutionJournal,
) -> ProveInput<'a> {
    ProveInput {
        snapshot,
        batch,
        context,
        executed,
    }
}

/// Build a single-tx enroll batch for [`relation_source`] programs.
pub fn enroll_batch(executor: &PreparedExecutor) -> ir::EntryBatch {
    tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }])
}

#[path = "prover_relation_tests/relation_trace_tests.rs"]
mod relation_trace_tests;
#[path = "prover_relation_tests/tampering_tests.rs"]
mod tampering_tests;
#[path = "prover_relation_tests/witness_labels_tests.rs"]
mod witness_labels_tests;
