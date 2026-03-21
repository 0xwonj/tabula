//! Shared execution-oriented helpers built on public compiler/executor seams.

use std::collections::BTreeMap;

use tabula_artifact::{Artifact, ArtifactError, State as ArtifactState, TransactionBatch};
use tabula_compiler::{
    CompilerCatalogs, SchemeDescriptorCatalog, SealedProgram, compile_program_source,
    register_artifact, register_program, register_program_definition,
    register_program_definition_with_catalogs,
};
use tabula_core::error::TabulaError;
use tabula_core::mock::Blake3Hasher;
use tabula_core::traits::StateView;
use tabula_core::{
    Batch, CellKey, ColId, InMemoryState, InMemoryStaticTables, NoopSigVerifier, RowKey, TableId,
    Transaction, TxTypeId, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::{Instruction, Program, PropertyQuery, TxTypeDef};
use tabula_lang::compile;

use crate::fixtures::schema::single_u64_column_schema;

pub fn program_from_source(source: &str) -> Program {
    let compiled = compile(source).expect("DSL compilation");
    let mut program = Program::new();
    for schema in &compiled.schemas {
        program.add_schema(schema.clone());
    }
    for tx in &compiled.tx_types {
        program.register(tx.clone()).expect("tx registration");
    }
    program
}

pub fn compiled_program_from_source(source: &str) -> SealedProgram {
    let definition = compile_program_source(source).expect("compile source");
    register_program_definition(&definition).expect("register compiled program")
}

pub fn compiled_program_from_artifact(artifact: &Artifact) -> SealedProgram {
    register_artifact(artifact).expect("register compiled artifact")
}

pub fn artifact_from_source(source: &str) -> Artifact {
    compiled_program_from_source(source).into_artifact()
}

pub fn artifact_from_source_with_catalog(
    source: &str,
    catalog: &SchemeDescriptorCatalog,
) -> Artifact {
    let definition = compile_program_source(source).expect("compile source");
    register_program_definition_with_catalogs(
        &definition,
        &CompilerCatalogs {
            schemes: catalog.clone(),
            precompiles: Default::default(),
        },
    )
    .expect("register source with scheme catalog")
    .into_artifact()
}

pub fn compiled_property_successor_program() -> SealedProgram {
    let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "scan".to_string(),
        param_schema: vec![],
        body: vec![Instruction::PropertyRead {
            dst_val: 0,
            dst_key: 1,
            dst_is_null: 2,
            table: TableId(1),
            col: ColId(0),
            query: PropertyQuery::Successor { key: RowKey(0) },
        }],
    };
    register_program(&[schema], &[tx]).expect("register property query program")
}

pub fn batch_from_transactions(transactions: Vec<Transaction>) -> Batch {
    Batch { transactions }
}

pub fn core_transactions_from_artifact_batch(
    batch: &TransactionBatch,
) -> Result<Vec<Transaction>, ArtifactError> {
    batch
        .transactions
        .iter()
        .map(tabula_artifact::TransactionInput::to_transaction)
        .collect()
}

pub fn core_batch_from_artifact_batch(batch: &TransactionBatch) -> Result<Batch, ArtifactError> {
    Ok(Batch {
        transactions: core_transactions_from_artifact_batch(batch)?,
    })
}

pub fn initial_cells_from_state(snapshot: &ArtifactState) -> Vec<(TableId, ColId, RowKey, Value)> {
    snapshot
        .cells
        .iter()
        .filter_map(|entry| {
            entry.value.map(|value| {
                (
                    TableId(entry.table),
                    ColId(entry.col),
                    RowKey(entry.row),
                    value,
                )
            })
        })
        .collect()
}

pub fn in_memory_state_from_cells(cells: &[(TableId, ColId, RowKey, Value)]) -> InMemoryState {
    let mut snapshot = InMemoryState::new();
    for &(table, col, row, value) in cells {
        snapshot.set(CellKey { table, col, row }, value);
    }
    snapshot
}

pub fn in_memory_state_from_state(snapshot: &ArtifactState) -> InMemoryState {
    let mut state = InMemoryState::new();
    for entry in &snapshot.cells {
        if let Some(value) = entry.value {
            state.set(
                CellKey {
                    table: TableId(entry.table),
                    col: ColId(entry.col),
                    row: RowKey(entry.row),
                },
                value,
            );
        }
    }
    state
}

pub fn execute_batch_with_defaults<S: StateView>(
    batch: &Batch,
    program: &Program,
    snapshot: &S,
) -> Result<tabula_core::BatchResult, TabulaError> {
    let static_tables = InMemoryStaticTables::new();
    let property_queries = PropertyQueryRegistry::new();
    let env = BatchEnv {
        hasher: &Blake3Hasher,
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &tabula_core::SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    execute_batch(batch, program, snapshot, &env, &BTreeMap::new())
}
