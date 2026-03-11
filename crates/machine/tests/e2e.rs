//! End-to-end STARK prover/verifier tests.
//!
//! Pipeline: DSL source → compile → execute → witness → shard → per-tier traces → prove → verify.

mod common;

use p3_field::PrimeCharacteristicRing;

use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_machine::{EF4, VerificationError};

use common::{make_tx, prove_and_verify, run_pipeline};

// ── E1: Basic prove/verify ──────────────────────────────────────────────────

#[test]
fn prove_verify_read_write() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

// ── E2: Statement consistency ───────────────────────────────────────────────

#[test]
fn statement_has_consistent_roots() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let statement = prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );

    // Identity transaction: read and write-back the same value.
    // old_root == new_root because state doesn't change.
    assert_eq!(
        statement.old_root, statement.new_root,
        "identity transaction should not change state root"
    );
}

// ── E3: Proof structure validation ──────────────────────────────────────────

#[test]
fn proof_has_expected_structure() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let pipeline = run_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );

    let proof = pipeline
        .machine
        .prove(
            pipeline.traces,
            &pipeline.column_identities,
            pipeline.statement,
        )
        .expect("proving");

    // C+2 architecture: 1 execution + C columns + 1 root.
    assert!(
        !proof.columns.is_empty(),
        "proof should have at least one column proof"
    );

    // Execution proof should have chip openings.
    assert!(
        !proof.execution.chip_openings.is_empty(),
        "execution proof should have chip openings"
    );

    // Root proof should have chip openings.
    assert!(
        !proof.root.chip_openings.is_empty(),
        "root proof should have chip openings"
    );

    // Verify passes.
    pipeline.machine.verify(&proof).expect("verification");
}

// ── E4: Multi-column — touched and untouched ────────────────────────────────

/// Multi-column scenario: table with 2 columns, transaction only touches one.
///
/// Tests:
/// - Separate old/new SMT siblings (SmtPathCols sibling split)
/// - Untouched column proof skipping (no column proof generated for `name`)
/// - SMT root consistency with mixed touched/untouched columns
#[test]
fn multi_column_touched_and_untouched() {
    let source = "\
table accounts { balance: u64, name: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let initial = &[
        (TableId(0), ColId(0), RowKey(10), Value::U64(50)),
        (TableId(0), ColId(1), RowKey(10), Value::U64(99)),
    ];
    let txs = vec![make_tx(vec![Value::U64(10)])];

    prove_and_verify(source, initial, txs);
}

// ── E5: Multi-column — all touched ──────────────────────────────────────────

/// Multi-column: all columns touched within the same table.
///
/// Tests that separate old/new SMT siblings handle multiple columns
/// changing simultaneously in the same table.
#[test]
fn multi_column_all_touched() {
    let source = "\
table accounts { balance: u64, score: u64 }
tx update(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
    let sc = accounts[id].score
    accounts[id].score = sc
}";
    let initial = &[
        (TableId(0), ColId(0), RowKey(5), Value::U64(100)),
        (TableId(0), ColId(1), RowKey(5), Value::U64(42)),
    ];
    let txs = vec![make_tx(vec![Value::U64(5)])];

    prove_and_verify(source, initial, txs);
}

// ── Negative tests: tampered proofs ─────────────────────────────────────────

/// Helper: produce a valid proof from the basic single-column pipeline.
fn make_valid_proof() -> (tabula_machine::TabulaMachine, tabula_machine::TabulaProof) {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let pipeline = run_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
    let proof = pipeline
        .machine
        .prove(
            pipeline.traces,
            &pipeline.column_identities,
            pipeline.statement,
        )
        .expect("proving");
    // Sanity: valid proof passes.
    pipeline.machine.verify(&proof).expect("baseline verify");
    (pipeline.machine, proof)
}

// ── N1: Tampered cross-proof bus cumsum ─────────────────────────────────────

/// Modifying an exported cumsum should break cross-proof bus balance.
#[test]
fn tampered_cumsum_rejected() {
    let (machine, mut proof) = make_valid_proof();

    // Flip a coefficient in the execution tier's exported cumsums.
    if let Some((_bus, cs)) = proof.execution.exported_cumsums.iter_mut().next() {
        *cs += EF4::ONE;
    }

    let err = machine.verify(&proof).unwrap_err();
    assert!(
        matches!(err, VerificationError::CrossProofBusImbalance { .. }),
        "expected CrossProofBusImbalance, got: {err}"
    );
}

// ── N2: Unknown column identity ────────────────────────────────────────────

/// A column proof with an identity not in the verifier setup must be rejected.
#[test]
fn unknown_column_identity_rejected() {
    let (machine, mut proof) = make_valid_proof();

    // Change the first column proof's identity to a nonexistent (table, col).
    assert!(!proof.columns.is_empty());
    proof.columns[0].identity.table_id = 999;
    proof.columns[0].identity.col_id = 999;

    let err = machine.verify(&proof).unwrap_err();
    assert!(
        matches!(
            err,
            VerificationError::ColumnIdentityMismatch {
                proof_table: 999,
                proof_col: 999,
                ..
            }
        ),
        "expected ColumnIdentityMismatch, got: {err}"
    );
}
