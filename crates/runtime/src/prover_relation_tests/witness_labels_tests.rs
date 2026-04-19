//! Witness-label contract tests.
//!
//! Tests that verify the relation-table chip public values, event-transcript
//! witness alignment, output-digest correctness for enum/map relations, and
//! relation-chip opening validation.

use super::{
    enroll_batch, event_debug_source, executor_and_prover_for_source, relation_context,
    relation_snapshot, relation_source,
};
use crate::verifier::relation_table_root_from_proof;
use crate::{ProveInput, prepare_executor};

use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
use tabula_contract::format::typed_tuple::{TypedTupleRole, compute_typed_tuple_digest};
use tabula_ir as ir;
use crate::semantics as runtime_ir;
use tabula_stark::trace::witness_labels;
use tabula_testing::exec::register_program_from_source;
use tabula_types::{u64_portable, u64_typed};

/// Extract event items from an instruction record slice, sorted by item index.
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
fn relation_table_rows_use_empty_output_digest_for_enum_relations() {
    let (registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let empty_digest = compute_typed_tuple_digest(TypedTupleRole::RelationOutput, &[])
        .expect("empty tuple digest");
    let allowed_rows = registered
        .static_table_artifact()
        .rows
        .iter()
        .filter(|row| row.relation_id == 0)
        .collect::<Vec<_>>();
    assert_eq!(allowed_rows.len(), 4);
    assert!(
        allowed_rows
            .iter()
            .all(|row| row.output_digest == empty_digest)
    );

    let chosen = allowed_rows[2];
    let proof_rows = tabula_witness::prepare_relation_proof(
        executor.state.semantic.execution().program(),
        registered.static_table_artifact(),
        &[tabula_witness::RelationClaim {
            relation: ir::RelationId(0),
            kind: tabula_witness::RelationClaimKind::Assert,
            inputs: vec![u64_typed(2)],
            input_digest: chosen.input_digest,
            outputs: vec![],
            output_digest: chosen.output_digest,
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            op_index: 0,
        }],
    )
    .expect("prepare relation proof rows");
    let rows = proof_rows
        .table_rows()
        .iter()
        .filter(|row| row.relation_id == 0)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.output_digest == empty_digest));
    assert_eq!(rows.iter().map(|row| row.lookup_mult).sum::<u32>(), 1);
}

#[test]
fn relation_proof_root_matches_registered_artifact_and_chip_public_values() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let proved = prover
        .prove_and_verify(
            &verifier,
            &ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            },
        )
        .expect("prove relation batch");
    let chip_root = relation_table_root_from_proof(&proved.proof, prover.machine())
        .expect("extract relation chip root");

    assert_eq!(
        prover.state.static_table_artifact.root,
        registered.static_table_artifact().root
    );
    assert_eq!(
        chip_root,
        Some(registered.static_table_artifact().root),
        "relation table chip root must match the registered artifact root",
    );
    assert_eq!(
        runtime_ir::compute_applied_tx_digest(
            &batch,
            prover.type_runtimes(),
            prover.encoding_runtimes(),
            &prover.state.tuple_encoding_defaults,
        )
        .expect("batch digest"),
        proved.public_statement.applied_tx_digest.to_bytes()
    );
    assert_eq!(
        executed.successful_txs().count(),
        1,
        "sanity-check proof came from the expected execution batch",
    );
}

#[test]
fn relation_chip_public_values_truncation_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    let relation_opening = proved
        .proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
        .expect("relation chip opening");
    relation_opening.public_values.pop();

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("truncated relation chip public values must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("machine metadata requires 8"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn relation_chip_public_values_append_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    let relation_opening = proved
        .proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
        .expect("relation chip opening");
    relation_opening.public_values.push(KoalaBear::ZERO);

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("extended relation chip public values must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("machine metadata requires 8"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn missing_relation_chip_opening_still_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    proved
        .proof
        .execution
        .chip_openings
        .retain(|opening| opening.chip_id != RELATION_TABLE_CHIP_ID);

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("missing relation chip opening must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("relation table chip opening is missing"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn event_transcript_witness_matches_execution_event_rows() {
    let registered = register_program_from_source(event_debug_source());
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover = crate::prepare_prover(Arc::new(registered), &opts).expect("build prover");
    let snapshot = executor.empty_state_snapshot();
    let register = executor
        .entry_id_by_symbol("register")
        .expect("register entry");
    let batch = tabula_testing::exec::tx_batch(vec![ir::EntryCall {
        entry_id: register,
        params: vec![u64_portable(1)],
    }]);
    let context = tabula_testing::exec::context_input([(ir::ContextFieldId(0), u64_portable(7))]);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute event batch");
    let state = &*executor.state;
    let typed_context =
        crate::prelude::decode_context_input_on_state(state, &context).expect("decode context");
    let typed_txs =
        crate::prelude::decode_entry_batch_on_state(state, &batch).expect("decode batch");

    let prepared = crate::proof_artifacts::prepare_proof_artifacts(
        &prover.state,
        &prover.root_backend_bundle,
        &prover.kit_registry,
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
