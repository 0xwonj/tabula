//! End-to-end STARK prover/verifier tests.
//!
//! Pipeline: DSL source -> compile -> execute -> witness -> trace bundle -> prove -> verify.

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
use tabula_witness::WitnessGenerator;
use tabula_witness::trace::build_trace_map;

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_baby_bear::BabyBear>)>>;

/// Compile DSL, execute a batch, generate witness, build traces, prove, and verify.
fn stark_pipeline(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) {
    // 1. Compile DSL source.
    let compiled = compile(source).expect("DSL compilation");
    let mut program = Program::new();
    for schema in &compiled.schemas {
        program.add_schema(schema.clone());
    }
    for tx in &compiled.tx_types {
        program.register(tx.clone()).expect("tx registration");
    }

    // 2. Execute the batch.
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

    // 3. Generate witness.
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

    // 4. Build trace map (all chip traces + public values).
    let traces = build_trace_map::<PoseidonHasher, 3>(
        &witness,
        &program,
        &batch,
        &result,
        &schemas_by_id,
        &InMemoryStaticTables::new(),
        PoseidonHasher::new(),
    )
    .expect("trace assembly");

    // 5. Prove.
    let statement = PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    };
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("machine build");
    let proof = machine.prove(&traces, statement).expect("proving");
    assert!(
        !proof.chip_proofs.is_empty(),
        "proof should contain at least one chip proof"
    );

    // 6. Verify.
    machine
        .verify(&proof)
        .expect("STARK verification should succeed");
}

fn make_tx(params: Vec<Value>) -> Transaction {
    make_tx_nonce(params, 0)
}

fn make_tx_nonce(params: Vec<Value>, nonce: u64) -> Transaction {
    Transaction {
        tx_type: TxTypeId(0),
        params,
        sender: [7u8; 32],
        nonce,
        signature: vec![],
    }
}

#[test]
fn stark_prove_verify_read_write() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    stark_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

#[test]
fn stark_prove_verify_arith() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";
    stark_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

#[test]
fn stark_prove_verify_cmp_assert() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    assert y >= x
    t[id].val = y - x
}";

    stark_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(5), Value::U64(100))],
        vec![make_tx(vec![Value::U64(5)])],
    );
}

#[test]
fn stark_prove_verify_multi_tx_batch() {
    // Two independent transactions in one batch, each touching a different key.
    let source = "\
table t { val: u64 }
tx bump(id: u64) {
    let x = t[id].val
    t[id].val = x + x
}";
    stark_pipeline(
        source,
        &[
            (TableId(0), ColId(0), RowKey(1), Value::U64(10)),
            (TableId(0), ColId(0), RowKey(2), Value::U64(20)),
        ],
        vec![
            make_tx_nonce(vec![Value::U64(1)], 0),
            make_tx_nonce(vec![Value::U64(2)], 1),
        ],
    );
}

#[test]
fn stark_prove_verify_mul() {
    // Multiplication opcode.
    let source = "\
table t { val: u64 }
tx square(id: u64) {
    let x = t[id].val
    t[id].val = x * x
}";
    stark_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(7))],
        vec![make_tx(vec![Value::U64(1)])],
    );
}

#[test]
fn stark_prove_verify_select() {
    // Conditional select opcode: pick between two read values.
    let source = "\
table t { val: u64 }
tx pick(id: u64, use_first: bool) {
    let x = t[id].val
    let result = select(use_first, x, 0)
    t[id].val = result
}";
    stark_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(42))],
        vec![Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(1), Value::Bool(true)],
            sender: [7u8; 32],
            nonce: 0,
            signature: vec![],
        }],
    );
}
