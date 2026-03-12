//! Trace-builder benchmarks.

use std::collections::BTreeMap;

use criterion::{Criterion, criterion_group, criterion_main};

use tabula_commitment::{BabyBearCodec, HybridVC, PoseidonHasher};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_witness::TraceBuilder;
use tabula_witness::WitnessGenerator;

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_baby_bear::BabyBear>)>>;

/// Shared setup: compile → execute → generate witness.
struct BenchSetup {
    witness: tabula_witness::BatchWitness<PoseidonHasher>,
    program: Program,
    batch: Batch,
    result: tabula_core::BatchResult,
    schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema>,
}

fn setup(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> BenchSetup {
    let compiled = compile(source).expect("DSL compilation");
    let mut program = Program::new();
    for schema in &compiled.schemas {
        program.add_schema(schema.clone());
    }
    for tx in &compiled.tx_types {
        program.register(tx.clone()).expect("tx registration");
    }

    let mut snapshot = InMemoryState::new();
    for &(table, col, row, value) in initial_cells {
        snapshot.set(CellKey { table, col, row }, value);
    }

    let batch = Batch {
        transactions: transactions.clone(),
    };
    let hasher = PoseidonHasher::new();
    let static_tables = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

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

    BenchSetup {
        witness,
        program,
        batch: Batch { transactions },
        result,
        schemas_by_id,
    }
}

fn make_tx(params: Vec<Value>) -> Transaction {
    Transaction {
        tx_type: TxTypeId(0),
        params,
        sender: [7u8; 32],
        nonce: 0,
        signature: vec![],
    }
}

fn bench_trace_read_write(c: &mut Criterion) {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let s = setup(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );

    c.bench_function("trace_read_write", |b| {
        b.iter(|| {
            let builder = TraceBuilder::<PoseidonHasher, 3>::new(&s.witness);
            let store = builder
                .prepare_witness_store(
                    &s.program,
                    &s.batch,
                    &s.result,
                    &s.schemas_by_id,
                    &InMemoryStaticTables::new(),
                    PoseidonHasher::new(),
                    None,
                )
                .unwrap();
            let chips = tabula_chips::core_dyn_chips();
            let consumers = tabula_chips::core_bus_consumers();
            tabula_witness::trace::build_all_traces(&chips, &consumers, store).unwrap();
        });
    });
}

fn bench_trace_arith(c: &mut Criterion) {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";
    let s = setup(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        vec![make_tx(vec![Value::U64(10)])],
    );

    c.bench_function("trace_arith", |b| {
        b.iter(|| {
            let builder = TraceBuilder::<PoseidonHasher, 3>::new(&s.witness);
            let store = builder
                .prepare_witness_store(
                    &s.program,
                    &s.batch,
                    &s.result,
                    &s.schemas_by_id,
                    &InMemoryStaticTables::new(),
                    PoseidonHasher::new(),
                    None,
                )
                .unwrap();
            let chips = tabula_chips::core_dyn_chips();
            let consumers = tabula_chips::core_bus_consumers();
            tabula_witness::trace::build_all_traces(&chips, &consumers, store).unwrap();
        });
    });
}

criterion_group!(benches, bench_trace_read_write, bench_trace_arith);
criterion_main!(benches);
