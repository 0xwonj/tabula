//! Shared test infrastructure for tabula-machine integration tests.
//!
//! Provides a full pipeline: DSL source → compile → execute → witness → shard → prove → verify.

#![allow(dead_code)]

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{BabyBearCodec, ColumnMeta, HybridVC, PoseidonHasher};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_machine::{ColumnIdentity, ColumnSetupConfig, PublicStatement, TabulaMachine};
use tabula_witness::trace::{partition_by_tier, prepare_shard_witness};
use tabula_witness::{TraceBuilder, WitnessGenerator};

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<BabyBear>)>>;

/// Pipeline output: everything needed for proving.
pub struct TestPipeline {
    pub machine: TabulaMachine,
    pub traces: tabula_machine::ProofTraces,
    pub column_identities: Vec<ColumnIdentity>,
    pub statement: PublicStatement,
}

/// Run the full pipeline: compile → execute → witness → shard → traces.
#[allow(clippy::needless_pass_by_value)]
pub fn run_pipeline(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> TestPipeline {
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

    let statement = PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    };

    // Build column configs from witness metadata.
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
            receives_commitment: meta.tag == tabula_commitment::scheme_tags::SSMC,
        })
        .collect();

    let machine = TabulaMachine::new(&col_configs).expect("machine creation");

    // Build shard witness and partition stores.
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
    let traces = machine.build_traces(stores).expect("trace building");

    let column_identities: Vec<ColumnIdentity> = column_metas
        .iter()
        .map(|(t, c, meta)| ColumnIdentity {
            table_id: t.0,
            col_id: c.0,
            com_old: meta.com_old.0,
            com_new: meta.com_new.0,
        })
        .collect();

    TestPipeline {
        machine,
        traces,
        column_identities,
        statement,
    }
}

/// Full pipeline: compile → execute → witness → shard → prove → verify.
pub fn prove_and_verify(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> PublicStatement {
    let pipeline = run_pipeline(source, initial_cells, transactions);

    let proof = pipeline
        .machine
        .prove(
            pipeline.traces,
            &pipeline.column_identities,
            pipeline.statement,
        )
        .expect("proving");

    assert!(
        !proof.columns.is_empty(),
        "proof should have at least one column proof"
    );

    pipeline
        .machine
        .verify(&proof)
        .expect("verification should succeed");

    proof.statement
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
