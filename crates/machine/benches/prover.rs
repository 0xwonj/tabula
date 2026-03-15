//! STARK prover/verifier benchmarks.
//!
//! Measures prove + verify time across workload sizes:
//! - Column scaling: 1, 2, 4, 8 columns (shows sharding parallelism)
//! - Transaction scaling: 1, 4, 16 tx per batch
//!
//! Run: `cargo bench --package tabula-machine --bench prover`

use std::collections::BTreeMap;
use std::fmt::Write;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use tabula_commitment::{ColumnMeta, ColumnState, KoalaBearCodec, PoseidonHasher, scheme_tags};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_core::{InMemoryState, InMemoryStaticTables, NoopSigVerifier, SequentialNonce};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_machine::{ColumnIdentity, ColumnSetupConfig, TabulaMachine};
use tabula_stark::air::statement::PublicStatement;
use tabula_witness::trace::{partition_by_tier, prepare_shard_witness};
use tabula_witness::{TraceBuilder, WitnessGenerator};

type EncodedColumnEntries =
    BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_koala_bear::KoalaBear>)>>;

// ── Workload generator ────────────────────────────────────────────────────

/// Generate DSL source with `num_tables` tables (each 1 column), each touched by its own tx.
///
/// Uses separate tables to avoid the MAX_SLOTS=16 limit per transaction.
/// Each table gets its own tx type so all columns are independently touched.
fn gen_source(num_tables: usize) -> String {
    let mut src = String::new();
    for i in 0..num_tables {
        writeln!(src, "table t{i} {{ val: u64 }}").unwrap();
    }
    for i in 0..num_tables {
        writeln!(src, "tx touch{i}(id: u64) {{").unwrap();
        writeln!(src, "    let v = t{i}[id].val").unwrap();
        writeln!(src, "    t{i}[id].val = v").unwrap();
        writeln!(src, "}}").unwrap();
    }
    src
}

/// Generate initial state: 1 row per table, `num_tables` tables.
fn gen_initial(num_tables: usize) -> Vec<(TableId, ColId, RowKey, Value)> {
    (0..num_tables)
        .map(|t| (TableId(t as u32), ColId(0), RowKey(0), Value::U64(100)))
        .collect()
}

/// Generate 1 transaction per table (each tx touches that table's column).
fn gen_txs(num_tables: usize) -> Vec<Transaction> {
    (0..num_tables)
        .map(|i| Transaction {
            tx_type: TxTypeId(i as u32),
            params: vec![Value::U64(0)],
            sender: [7u8; 32],
            nonce: i as u64,
            signature: vec![],
        })
        .collect()
}

/// Generate DSL source with 1 table, `num_cols` columns (max 8), touched by 1 tx.
fn gen_source_multi_col(num_cols: usize) -> String {
    assert!(
        num_cols <= 8,
        "MAX_SLOTS=16, 2 slots/col → max 8 cols per tx"
    );
    let mut cols = String::new();
    for i in 0..num_cols {
        if i > 0 {
            cols.push_str(", ");
        }
        write!(cols, "c{i}: u64").unwrap();
    }
    let mut body = String::new();
    for i in 0..num_cols {
        writeln!(body, "    let v{i} = t[id].c{i}").unwrap();
        writeln!(body, "    t[id].c{i} = v{i}").unwrap();
    }
    format!("table t {{ {cols} }}\ntx touch(id: u64) {{\n{body}}}")
}

/// Generate initial state for multi-col single-table workload.
fn gen_initial_multi_col(num_cols: usize, num_rows: usize) -> Vec<(TableId, ColId, RowKey, Value)> {
    let mut cells = Vec::new();
    for c in 0..num_cols {
        for r in 0..num_rows {
            cells.push((
                TableId(0),
                ColId(c as u16),
                RowKey(r as u64),
                Value::U64(100 + r as u64),
            ));
        }
    }
    cells
}

/// Generate `num_tx` transactions for single-table workload.
fn gen_txs_single_table(num_tx: usize) -> Vec<Transaction> {
    (0..num_tx)
        .map(|i| Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(i as u64)],
            sender: [7u8; 32],
            nonce: i as u64,
            signature: vec![],
        })
        .collect()
}

// ── Pipeline builder ──────────────────────────────────────────────────────

/// Pipeline output for benchmarks.
struct BenchPipeline {
    machine: TabulaMachine,
    traces: tabula_machine::ProofTraces,
    column_identities: Vec<ColumnIdentity>,
    statement: PublicStatement,
}

/// Build the full pipeline: compile → execute → witness → trace.
#[allow(clippy::needless_pass_by_value)]
fn build_pipeline(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> BenchPipeline {
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
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_openings: None,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

    let commit_hasher = PoseidonHasher::new();
    let codec = KoalaBearCodec;

    let mut entries_by_col: EncodedColumnEntries = BTreeMap::new();
    for &(table, col, row, value) in initial_cells {
        entries_by_col
            .entry((table, col))
            .or_default()
            .push((row, codec.encode(&value).expect("encode")));
    }

    let mut old_column_states = BTreeMap::new();
    let schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema> = compiled
        .schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    for schema in &compiled.schemas {
        for col_def in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) = ColumnState::commit(
                &commit_hasher,
                schema.id,
                col_def.id,
                entries,
                scheme_tags::SSMC,
            )
            .unwrap();
            old_column_states.insert((schema.id, col_def.id), state);
        }
    }

    let wg = WitnessGenerator::new(PoseidonHasher::new());
    let witness = wg
        .generate(&result, &schemas_by_id, &old_column_states)
        .expect("witness generation");

    let statement = PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    };

    let column_metas: Vec<(TableId, ColId, ColumnMeta)> = witness
        .columns
        .iter()
        .map(|col| (col.table, col.col, col.meta.clone()))
        .collect();

    let col_configs: Vec<ColumnSetupConfig> = column_metas
        .iter()
        .map(|(t, c, meta)| ColumnSetupConfig {
            table_id: *t,
            col_id: *c,
            scheme_tag: meta.tag,
            receives_commitment: meta.tag == scheme_tags::SSMC,
        })
        .collect();

    let machine = TabulaMachine::new(&col_configs).expect("machine build");

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

    let shard_witness =
        prepare_shard_witness::<PoseidonHasher, 3>(&witness).expect("shard witness preparation");

    let stores = partition_by_tier(store, shard_witness);
    let traces = machine.build_traces(stores).expect("trace assembly");

    let column_identities: Vec<ColumnIdentity> = column_metas
        .iter()
        .map(|(t, c, meta)| ColumnIdentity {
            table_id: t.0,
            col_id: c.0,
            com_old: meta.com_old.0,
            com_new: meta.com_new.0,
        })
        .collect();

    BenchPipeline {
        machine,
        traces,
        column_identities,
        statement,
    }
}

// ── Benchmark: column scaling ─────────────────────────────────────────────

fn bench_prove_column_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("prove");
    group.sample_size(10);

    for num_cols in [1, 2, 4, 8, 16, 32, 64, 128] {
        let source = gen_source(num_cols);
        let initial = gen_initial(num_cols, 1);
        let txs = gen_txs(1);
        let pipeline = build_pipeline(&source, &initial, txs);

        group.bench_with_input(BenchmarkId::new("columns", num_cols), &num_cols, |b, _| {
            b.iter(|| {
                pipeline
                    .machine
                    .prove(
                        pipeline.traces.clone(),
                        &pipeline.column_identities,
                        pipeline.statement.clone(),
                    )
                    .expect("proving");
            });
        });
    }

    group.finish();
}

fn bench_verify_column_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");

    for num_cols in [1, 2, 4, 8, 16, 32, 64, 128] {
        let source = gen_source(num_cols);
        let initial = gen_initial(num_cols, 1);
        let txs = gen_txs(1);
        let pipeline = build_pipeline(&source, &initial, txs);

        let proof = pipeline
            .machine
            .prove(
                pipeline.traces,
                &pipeline.column_identities,
                pipeline.statement,
            )
            .expect("proving");

        group.bench_with_input(BenchmarkId::new("columns", num_cols), &num_cols, |b, _| {
            b.iter(|| {
                pipeline.machine.verify(&proof).expect("verification");
            });
        });
    }

    group.finish();
}

// ── Benchmark: transaction scaling ────────────────────────────────────────

fn bench_prove_tx_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("prove");
    group.sample_size(10);

    for num_tx in [1, 4, 16] {
        let source = gen_source(1);
        let initial = gen_initial(1, num_tx);
        let txs = gen_txs(num_tx);
        let pipeline = build_pipeline(&source, &initial, txs);

        group.bench_with_input(BenchmarkId::new("transactions", num_tx), &num_tx, |b, _| {
            b.iter(|| {
                pipeline
                    .machine
                    .prove(
                        pipeline.traces.clone(),
                        &pipeline.column_identities,
                        pipeline.statement.clone(),
                    )
                    .expect("proving");
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_prove_column_scaling,
    bench_verify_column_scaling,
    bench_prove_tx_scaling,
);
criterion_main!(benches);
