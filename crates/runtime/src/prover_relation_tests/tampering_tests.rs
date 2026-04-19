//! Class-B tamper tests.
//!
//! These tests construct chip-row values directly to verify that the prover
//! correctly rejects tampered witness data. They are allowed to name chip-layer
//! types (guarded by the `_tests.rs` suffix exemption in the F0 chip-row
//! boundary guardrail).

use super::{
    enroll_batch, executor_and_prover_for_source, prove_input, relation_context, relation_snapshot,
    relation_source,
};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::relation_transcript::{
    RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
};
use tabula_machine::BackendProver;
use tabula_stark::trace::witness_labels;

#[test]
fn tampering_relation_table_rows_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.state,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
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
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.state,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
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
fn tampering_relation_effect_identity_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = enroll_batch(&executor);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.state,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

    let mut calls = machine_input
        .execution
        .store
        .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
        .expect("relation transcript calls")
        .clone();
    assert!(
        calls.len() >= 4,
        "expected multiple relation transcript calls"
    );
    calls[2].effect_ordinal_in_tx = calls[0].effect_ordinal_in_tx;
    machine_input
        .execution
        .store
        .put(RELATION_TRANSCRIPT_WITNESS_LABEL, calls);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation effect identity must fail proving"
    );
}
