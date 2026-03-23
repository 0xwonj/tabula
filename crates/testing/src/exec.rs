//! Shared execution-oriented helpers built on public compiler/executor seams.

use std::collections::BTreeMap;

use tabula_artifact::{Artifact, ArtifactError, State as ArtifactState, TransactionBatch};
use tabula_compiler::{
    CompilerCatalogs, ProgramDefinition, SealedProgram, SourceTableSchema, compile_program_source,
    register_artifact, register_program_definition, register_program_definition_with_catalogs,
};
use tabula_core::error::TabulaError;
use tabula_core::mock::Blake3Hasher;
use tabula_core::traits::StateView;
use tabula_core::{
    Batch, CellKey, ColId, InMemoryState, InMemoryStaticTables, NoopSigVerifier, PortableValue,
    RowKey, TableId, Transaction, TxTypeId,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{ResolvedExecutionProgram, derive_batch_report};
use tabula_ir::{Instruction, Program, PropertyQuery, TxTypeDef};
use tabula_profile::SemanticRegistry;
use tabula_types::TypeRuntimeRegistry;

use crate::fixtures::schema::single_u64_column_schema;

pub fn program_from_source(source: &str) -> Program {
    compiled_program_from_source(source).program().clone()
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

pub fn artifact_from_source_with_registry(source: &str, registry: &SemanticRegistry) -> Artifact {
    let definition = compile_program_source(source).expect("compile source");
    let catalogs = CompilerCatalogs::standard()
        .with_semantic_registry(registry.clone())
        .expect("semantic registry");
    register_program_definition_with_catalogs(&definition, &catalogs)
        .expect("register source with semantic registry")
        .into_artifact()
}

pub fn compiled_program_from_definition(
    table_schemas: Vec<SourceTableSchema>,
    tx_types: Vec<TxTypeDef>,
) -> SealedProgram {
    register_program_definition(&ProgramDefinition {
        table_schemas,
        tx_types,
        column_schemes: vec![],
    })
    .expect("register program definition")
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
    compiled_program_from_definition(vec![schema], vec![tx])
}

pub fn batch_from_transactions(transactions: Vec<Transaction>) -> Batch {
    Batch { transactions }
}

pub fn core_transactions_from_artifact_batch(
    batch: &TransactionBatch,
) -> Result<Vec<Transaction>, ArtifactError> {
    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
    batch
        .transactions
        .iter()
        .map(|tx| tx.to_transaction(&type_runtimes))
        .collect()
}

pub fn core_batch_from_artifact_batch(batch: &TransactionBatch) -> Result<Batch, ArtifactError> {
    Ok(Batch {
        transactions: core_transactions_from_artifact_batch(batch)?,
    })
}

pub fn initial_cells_from_state(
    snapshot: &ArtifactState,
) -> Vec<(TableId, ColId, RowKey, PortableValue)> {
    snapshot
        .cells
        .iter()
        .filter_map(|entry| {
            entry.value.as_ref().map(|value| {
                (
                    TableId(entry.table),
                    ColId(entry.col),
                    RowKey(entry.row),
                    value.clone(),
                )
            })
        })
        .collect()
}

pub fn in_memory_state_from_cells(
    cells: &[(TableId, ColId, RowKey, PortableValue)],
) -> InMemoryState {
    let mut snapshot = InMemoryState::new();
    for (table, col, row, value) in cells {
        snapshot.set(
            CellKey {
                table: *table,
                col: *col,
                row: *row,
            },
            value.clone(),
        );
    }
    snapshot
}

pub fn in_memory_state_from_state(snapshot: &ArtifactState) -> InMemoryState {
    let mut state = InMemoryState::new();
    for entry in &snapshot.cells {
        if let Some(value) = &entry.value {
            state.set(
                CellKey {
                    table: TableId(entry.table),
                    col: ColId(entry.col),
                    row: RowKey(entry.row),
                },
                value.clone(),
            );
        }
    }
    state
}

pub fn execute_batch_with_defaults<S: StateView>(
    batch: &Batch,
    program: &Program,
    snapshot: &S,
) -> Result<tabula_core::BatchReport, TabulaError> {
    let static_tables = InMemoryStaticTables::new();
    let property_queries = PropertyQueryRegistry::new();
    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
    let resolved = ResolvedExecutionProgram::from_program(program)?;
    let env = BatchEnv {
        hasher: &Blake3Hasher,
        type_runtimes: &type_runtimes,
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &tabula_core::SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let journal = execute_batch(batch, &resolved, snapshot, &env, &BTreeMap::new())?;
    derive_batch_report(&journal, &type_runtimes)
}
