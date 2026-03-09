//! Shared test infrastructure for tabula-machine integration tests.
//!
//! Provides a full pipeline: DSL source → compile → execute → witness → trace → prove → verify.

use std::collections::BTreeMap;

use tabula_commitment::{BabyBearCodec, HybridVC, PoseidonHasher};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_machine::TabulaMachine;
use tabula_stark::air::statement::PublicStatement;
use tabula_witness::{TraceBuilder, WitnessGenerator};

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_baby_bear::BabyBear>)>>;

/// Build a default machine (8 core + 1 commitment = 9 chips).
pub fn default_machine() -> TabulaMachine {
    TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("machine build")
}

/// Execute a batch and generate witness data.
///
/// Returns `(traces, statement)` ready for proving.
pub fn build_traces_from_source(
    machine: &TabulaMachine,
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> (tabula_witness::trace::TraceMap, PublicStatement) {
    let compiled = compile(source).expect("DSL compilation");
    let mut program = Program::new();
    for schema in &compiled.schemas {
        program.add_schema(schema.clone());
    }
    for tx in &compiled.tx_types {
        program.register(tx.clone()).expect("tx registration");
    }

    // Execute the batch.
    let mut snapshot = InMemoryState::new();
    for &(table, col, row, value) in initial_cells {
        snapshot.set(CellKey { table, col, row }, value);
    }

    let batch = Batch { transactions };
    let hasher = PoseidonHasher::new();
    let static_tables = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

    // Generate witness.
    let vc = HybridVC::new(PoseidonHasher::new(), 1024);
    let codec = BabyBearCodec;

    let mut entries_by_col: EncodedColumnEntries = BTreeMap::new();
    for &(table, col, row, value) in initial_cells {
        entries_by_col
            .entry((table, col))
            .or_default()
            .push((row, codec.encode(&value).expect("encode")));
    }

    let mut old_column_states = BTreeMap::new();
    for schema in &compiled.schemas {
        for col_def in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) = vc.commit_column(schema.id, col_def.id, entries).unwrap();
            old_column_states.insert((schema.id, col_def.id), state);
        }
    }

    let schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema> = compiled
        .schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    let wg = WitnessGenerator::new(vc);
    let witness = wg
        .generate(&result, &schemas_by_id, &old_column_states)
        .expect("witness generation");

    // Build traces.
    let store = TraceBuilder::<PoseidonHasher, 3>::new(&witness)
        .prepare_witness_store(
            &program,
            &batch,
            &result,
            &schemas_by_id,
            &InMemoryStaticTables::new(),
            PoseidonHasher::new(),
        )
        .expect("witness store preparation");
    let traces = machine.build_traces(store).expect("trace assembly");

    let statement = PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    };

    (traces, statement)
}

/// Full pipeline: compile → execute → witness → trace → prove → verify.
pub fn prove_and_verify(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) {
    let machine = default_machine();
    let (traces, statement) = build_traces_from_source(&machine, source, initial_cells, transactions);

    let proof = machine.prove(&traces, statement).expect("proving");
    assert_eq!(
        proof.chip_openings.len(),
        machine.registry().chip_ids().len(),
        "proof should contain one opening per registered chip"
    );

    machine
        .verify(&proof)
        .expect("STARK verification should succeed");
}

/// Create a transaction with nonce=0.
pub fn make_tx(params: Vec<Value>) -> Transaction {
    make_tx_nonce(params, 0)
}

/// Create a transaction with a specific nonce.
pub fn make_tx_nonce(params: Vec<Value>, nonce: u64) -> Transaction {
    Transaction {
        tx_type: TxTypeId(0),
        params,
        sender: [7u8; 32],
        nonce,
        signature: vec![],
    }
}
