//! Trace-builder benchmarks.

use std::collections::BTreeMap;

use criterion::{Criterion, criterion_group, criterion_main};
use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnMeta, ColumnState, KoalaBearCodec, PoseidonHasher, scheme_tags};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_core::{InMemoryState, InMemoryStaticTables, NoopSigVerifier, SequentialNonce};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_witness::trace::builtin::lowering::lower_program_batch;
use tabula_witness::{
    BuiltinTraceBuilder, BuiltinTraceContext, ExecutionInputPreparer, proof_column_commitment,
};

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<KoalaBear>)>>;

struct BenchSetup {
    column_metas: Vec<ColumnMeta>,
    old_state_root: tabula_commitment::NativeDigest,
    new_state_root: tabula_commitment::NativeDigest,
    program: Program,
    batch: Batch,
    result: tabula_core::BatchResult,
    schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema>,
}

fn build_ssmc_meta(
    table: TableId,
    col: ColId,
    old_state: &ColumnState<PoseidonHasher>,
    writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    is_touched: bool,
) -> ColumnMeta {
    let hasher = PoseidonHasher::new();
    let com_old = proof_column_commitment(table, col, old_state).expect("old commitment");
    let tag = old_state.scheme_tag();
    let is_empty_old = old_state.is_empty();
    let (new_state, _, _) = if is_touched {
        old_state.apply_writes(&hasher, table, col, writes)
    } else {
        (old_state.clone(), com_old, None)
    };
    let com_new = proof_column_commitment(table, col, &new_state).expect("new commitment");

    ColumnMeta {
        table,
        col,
        tag,
        com_old,
        com_new,
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched,
    }
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
    let property_queries = PropertyQueryRegistry::new();
    let env = BatchEnv {
        hasher: &hasher,
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

    let schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema> = compiled
        .schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    let planned_columns: Vec<(TableId, ColId)> = schemas_by_id
        .iter()
        .flat_map(|(table, schema)| schema.columns.iter().map(move |col| (*table, col.id)))
        .collect();
    let preparer = ExecutionInputPreparer::new(PoseidonHasher::new());
    let prepared = preparer
        .prepare_execution_inputs(&result, &schemas_by_id, planned_columns.iter())
        .expect("prepared execution inputs");

    let codec = KoalaBearCodec;
    let mut entries_by_col: EncodedColumnEntries = BTreeMap::new();
    for &(table, col, row, value) in initial_cells {
        entries_by_col
            .entry((table, col))
            .or_default()
            .push((row, codec.encode(&value).expect("encode")));
    }

    let mut metas = Vec::new();
    for schema in &compiled.schemas {
        for col_def in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (old_state, _) = ColumnState::commit(
                &PoseidonHasher::new(),
                schema.id,
                col_def.id,
                entries,
                scheme_tags::SSMC,
            )
            .unwrap();
            let writes = prepared
                .writes_by_col
                .get(&(schema.id, col_def.id))
                .cloned()
                .unwrap_or_default();
            metas.push(build_ssmc_meta(
                schema.id,
                col_def.id,
                &old_state,
                &writes,
                prepared.touched.contains(&(schema.id, col_def.id)),
            ));
        }
    }
    let (old_state_root, new_state_root) = preparer.compute_state_roots_from_metas(&metas);

    BenchSetup {
        column_metas: metas,
        old_state_root,
        new_state_root,
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

fn lower_for_setup(setup: &BenchSetup) -> tabula_witness::trace::builtin::lowering::LoweringOutput {
    let empty_columns: std::collections::BTreeSet<(TableId, ColId)> = setup
        .column_metas
        .iter()
        .filter(|meta| meta.is_empty_old)
        .map(|meta| (meta.table, meta.col))
        .collect();

    lower_program_batch::<3>(
        &setup.program,
        &setup.batch,
        &setup.result,
        &setup.schemas_by_id,
        &InMemoryStaticTables::new(),
        &empty_columns,
    )
    .expect("IR lowering")
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
            let builder = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
                column_metas: &s.column_metas,
                old_state_root: &s.old_state_root,
                new_state_root: &s.new_state_root,
            });
            let lowering = lower_for_setup(&s);
            let store = builder
                .prepare_witness_store(&lowering, PoseidonHasher::new())
                .unwrap()
                .store;
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
            let builder = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
                column_metas: &s.column_metas,
                old_state_root: &s.old_state_root,
                new_state_root: &s.new_state_root,
            });
            let lowering = lower_for_setup(&s);
            let store = builder
                .prepare_witness_store(&lowering, PoseidonHasher::new())
                .unwrap()
                .store;
            let chips = tabula_chips::core_dyn_chips();
            let consumers = tabula_chips::core_bus_consumers();
            tabula_witness::trace::build_all_traces(&chips, &consumers, store).unwrap();
        });
    });
}

criterion_group!(benches, bench_trace_read_write, bench_trace_arith);
criterion_main!(benches);
