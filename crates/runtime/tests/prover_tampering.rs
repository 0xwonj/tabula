//! External prove-path tests that fundamentally depend on chip-row
//! internals (`InstructionRecord`, `RelationTableWitnessRow`).
//!
//! These tests were moved out of `crates/runtime/src/prover_relation_tests.rs`
//! as part of SP-5 Fix F0b to restore the §8 chip-row boundary: runtime
//! production code no longer names chip row types. The tests live here —
//! under `crates/runtime/tests/` — because they tamper with chip-layer
//! witness-store contents between `prepare_proof_machine_input` and the
//! backend prover, which requires reading and mutating chip row types
//! directly. They cannot move to `tabula-chips` because they drive
//! runtime's prove surface (`PreparedProver`, `ProveInput`,
//! `prepare_proof_artifacts`).
//!
//! Runtime APIs consumed here are `#[doc(hidden)]` and exist to support
//! this style of test. They are not part of the stable surface.

#![cfg(feature = "prove")]
#![allow(missing_docs)]

use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
use tabula_ir as ir;
use tabula_machine::BackendProver;
use tabula_runtime::{PreparedOptions, ProveInput, prepare_executor, prepare_prover};
use tabula_stark::trace::witness_labels;
use tabula_testing::exec::{context_input, register_program_from_source, tx_batch};
use tabula_types::{bool_portable, u64_portable};

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

fn guarded_relation_source() -> &'static str {
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

fn event_debug_source() -> &'static str {
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

fn relation_context(caller: u64, epoch: u64) -> ir::ContextInput {
    context_input([
        (ir::ContextFieldId(0), u64_portable(caller)),
        (ir::ContextFieldId(1), u64_portable(epoch)),
    ])
}

fn guarded_context(caller: u64) -> ir::ContextInput {
    context_input([(ir::ContextFieldId(0), u64_portable(caller))])
}

fn relation_snapshot(
    registered: &tabula_compiler::RegisteredProgram,
) -> tabula_runtime::CommittedStateSnapshot {
    let opts = PreparedOptions::try_standard().expect("standard options");
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

fn executor_and_prover_for_source(
    source: &str,
) -> (
    tabula_compiler::RegisteredProgram,
    tabula_runtime::PreparedExecutor,
    tabula_runtime::PreparedProver,
) {
    let registered = register_program_from_source(source);
    let opts = PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover =
        prepare_prover(Arc::new(registered.clone()), &opts).expect("build prepared prover");
    (registered, executor, prover)
}

fn entry_id_for(executor: &tabula_runtime::PreparedExecutor, symbol: &str) -> ir::EntryId {
    executor
        .entry_id_by_symbol(symbol)
        .unwrap_or_else(|| panic!("missing entry '{symbol}'"))
}

fn extract_event_items(records: &[InstructionRecord]) -> Vec<(u32, [KoalaBear; 8])> {
    let mut items = records
        .iter()
        .filter_map(|record| match record.opcode {
            Opcode::EmitEventHeader => Some((
                record.proof_meta0.expect("event header item index"),
                [
                    KoalaBear::ONE,
                    KoalaBear::new(record.tx_index),
                    KoalaBear::new(
                        record
                            .instruction_index
                            .expect("event header instruction index"),
                    ),
                    KoalaBear::new(record.proof_meta1.expect("event header ordinal")),
                    KoalaBear::new(record.proof_meta2.expect("event header id")),
                    KoalaBear::new(record.proof_meta3.expect("event header arg count")),
                    KoalaBear::ZERO,
                    KoalaBear::ZERO,
                ],
            )),
            Opcode::EmitEventArg => Some((
                record.proof_meta0.expect("event arg item index"),
                [
                    KoalaBear::new(2),
                    KoalaBear::new(record.tx_index),
                    KoalaBear::new(record.proof_meta1.expect("event arg ordinal")),
                    KoalaBear::new(record.proof_meta2.expect("event arg index")),
                    KoalaBear::new(record.proof_meta3.expect("event arg type id")),
                    *record.src1_val.first().expect("event arg limb 0"),
                    *record.src1_val.get(1).expect("event arg limb 1"),
                    *record.src1_val.get(2).expect("event arg limb 2"),
                ],
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    items.sort_unstable_by_key(|(item_index, _)| *item_index);
    items
}

#[test]
fn untaken_relation_branches_emit_no_relation_claims_or_positive_lookup_counts() {
    use tabula_chips::relation_transcript::{
        RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
    };

    let (registered, executor, prover) = executor_and_prover_for_source(guarded_relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "maybe_promote"),
        params: vec![bool_portable(false), u64_portable(0), u64_portable(2)],
    }]);
    let context = guarded_context(7);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute guarded batch");

    let input = ProveInput::new(&snapshot, &batch, &context, &executed);
    let (machine_input, _public_statement) = tabula_runtime::proof_artifacts::prepare_proof_machine_input(
        prover.prepared_state(),
        prover.root_backend_bundle(),
        prover.kit_registry(),
        &input,
    )
    .expect("prepare proof request");

    let transcript_calls = machine_input
        .execution
        .store
        .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
        .expect("relation transcript calls");
    let lookup_rows = machine_input
        .execution
        .store
        .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
        .expect("relation lookup rows");

    assert!(transcript_calls.is_empty());
    assert!(
        lookup_rows.iter().all(|row| row.lookup_mult == 0),
        "untaken branches must not contribute positive relation lookup multiplicities",
    );
}

#[test]
fn tampering_relation_table_rows_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let input = ProveInput::new(&snapshot, &batch, &context, &executed);
    let (mut machine_input, _public_statement) =
        tabula_runtime::proof_artifacts::prepare_proof_machine_input(
            prover.prepared_state(),
            prover.root_backend_bundle(),
            prover.kit_registry(),
            &input,
        )
        .expect("prepare proof request");

    let mut rows = machine_input
        .execution
        .store
        .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
        .expect("relation lookup rows")
        .clone();
    assert!(!rows.is_empty(), "expected relation lookup rows");
    let tampered = rows
        .iter_mut()
        .find(|row| row.lookup_mult > 0)
        .expect("expected at least one consumed relation lookup row");
    tampered.output_digest[0] = tampered.output_digest[0].wrapping_add(1);
    machine_input
        .execution
        .store
        .put(RELATION_TABLE_WITNESS_LABEL, rows);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation lookup rows must fail proving"
    );
}

#[test]
fn tampering_execution_bound_relation_outputs_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let input = ProveInput::new(&snapshot, &batch, &context, &executed);
    let (mut machine_input, _public_statement) =
        tabula_runtime::proof_artifacts::prepare_proof_machine_input(
            prover.prepared_state(),
            prover.root_backend_bundle(),
            prover.kit_registry(),
            &input,
        )
        .expect("prepare proof request");

    let mut records = machine_input
        .execution
        .store
        .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
        .expect("execution records")
        .clone();
    let eval_record = records
        .iter_mut()
        .find(|record| record.opcode == Opcode::RelationProof && record.relation_is_eval)
        .expect("relation eval execution record");
    eval_record.relation_output_vals[0][0] += KoalaBear::ONE;

    machine_input
        .execution
        .store
        .put(witness_labels::EXECUTION_RECORDS, records);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation output binding must fail proving"
    );
}

#[test]
fn event_transcript_witness_matches_execution_event_rows() {
    let registered = register_program_from_source(event_debug_source());
    let opts = PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover = prepare_prover(Arc::new(registered), &opts).expect("build prover");
    let snapshot = executor.empty_state_snapshot();
    let register = executor
        .entry_id_by_symbol("register")
        .expect("register entry");
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: register,
        params: vec![u64_portable(1)],
    }]);
    let context = context_input([(ir::ContextFieldId(0), u64_portable(7))]);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute event batch");
    let state = prover.prepared_state();
    let typed_context = tabula_runtime::prelude::decode_context_input_on_state(state, &context)
        .expect("decode context");
    let typed_txs =
        tabula_runtime::prelude::decode_entry_batch_on_state(state, &batch).expect("decode batch");

    let prepared = tabula_runtime::proof_artifacts::prepare_proof_artifacts(
        prover.prepared_state(),
        prover.root_backend_bundle(),
        prover.kit_registry(),
        &snapshot,
        &typed_txs,
        &typed_context,
        &executed,
    )
    .expect("prepare proof artifacts");

    let records = prepared
        .execution
        .store
        .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
        .expect("execution records");
    let transcript_items = prepared
        .execution
        .store
        .get::<Vec<[KoalaBear; 8]>>(EVENT_TRANSCRIPT_WITNESS_LABEL)
        .expect("event transcript items");

    let execution_items = extract_event_items(records);
    let witness_items = transcript_items
        .iter()
        .copied()
        .enumerate()
        .map(|(index, block)| (index as u32, block))
        .collect::<Vec<_>>();

    assert_eq!(execution_items, witness_items);
}
