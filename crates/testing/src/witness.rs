use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use crate::exec::{in_memory_state_from_cells, program_from_source};
use crate::fixtures::cases::TraceCase;

use tabula_commitment::schemes::tags;
use tabula_commitment::{
    ColumnMeta, ColumnState, KoalaBearCodec, NativeDigest, PoseidonHasher,
    compute_state_roots_from_metas,
};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{
    Batch, ColId, InMemoryStaticTables, NoopSigVerifier, RowKey, SequentialNonce, TableId,
    TableSchema, Transaction, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_stark::air::interaction::core_buses;
use tabula_stark::trace::{TraceMap, WitnessStore, build_all_traces, debug_validate_trace_map};
use tabula_witness::stark::{
    LoweringOutput, SharedStoreBuilder, SharedStoreContext, lower_program_batch,
};
use tabula_witness::{ExecutionInputPreparer, PreparedExecutionColumns};

type EncodedColumnEntry = (RowKey, Vec<KoalaBear>);
type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<EncodedColumnEntry>>;
type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Shared STARK witness/trace harness used by tests, benches, and E2E checks.
pub struct StarkTraceHarness {
    pub program: Program,
    pub batch: Batch,
    pub result: tabula_core::BatchResult,
    pub column_metas: Vec<ColumnMeta>,
    pub old_state_root: NativeDigest,
    pub new_state_root: NativeDigest,
    pub schemas_by_id: BTreeMap<TableId, TableSchema>,
}

impl StarkTraceHarness {
    pub fn shared_store_context(&self) -> SharedStoreContext<'_> {
        SharedStoreContext {
            column_metas: &self.column_metas,
            old_state_root: &self.old_state_root,
            new_state_root: &self.new_state_root,
        }
    }

    pub fn empty_columns(&self) -> BTreeSet<(TableId, ColId)> {
        self.column_metas
            .iter()
            .filter(|meta| meta.is_empty_old)
            .map(|meta| (meta.table, meta.col))
            .collect()
    }
}

pub fn compile_execute_case(case: &TraceCase) -> StarkTraceHarness {
    compile_execute_context_impl(case.source, &case.initial_cells, case.transactions.clone())
}

pub fn compile_execute_context(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> StarkTraceHarness {
    compile_execute_context_impl(source, initial_cells, transactions)
}

pub fn prepare_execution_inputs(
    harness: &StarkTraceHarness,
) -> Result<PreparedExecutionColumns, TabulaError> {
    let planned_columns: Vec<(TableId, ColId)> = harness
        .schemas_by_id
        .iter()
        .flat_map(|(table, schema)| schema.columns.iter().map(move |column| (*table, column.id)))
        .collect();
    ExecutionInputPreparer::new().prepare_execution_inputs(
        &harness.result,
        &harness.schemas_by_id,
        planned_columns.iter(),
    )
}

fn compile_execute_context_impl(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> StarkTraceHarness {
    let program = program_from_source(source);
    let compiled_schemas: Vec<TableSchema> = program.schemas().values().cloned().collect();
    let snapshot = in_memory_state_from_cells(initial_cells);
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

    let schemas_by_id: BTreeMap<TableId, TableSchema> = compiled_schemas
        .iter()
        .cloned()
        .map(|schema| (schema.id, schema))
        .collect();
    let planned_columns: Vec<(TableId, ColId)> = schemas_by_id
        .iter()
        .flat_map(|(table, schema)| schema.columns.iter().map(move |column| (*table, column.id)))
        .collect();
    let preparer = ExecutionInputPreparer::new();
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
    let hasher = PoseidonHasher::new();
    for schema in &compiled_schemas {
        for column in &schema.columns {
            let prepared_column = prepared
                .columns
                .iter()
                .find(|prepared_column| {
                    prepared_column.table == schema.id && prepared_column.col == column.id
                })
                .expect("prepared column");
            let mut entries = entries_by_col
                .remove(&(schema.id, column.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (old_state, _) =
                ColumnState::commit(&hasher, schema.id, column.id, entries, tags::SSMC)
                    .expect("commit old state");
            let encoded_writes =
                encode_writes(&codec, &prepared_column.writes).expect("encode writes");
            metas.push(build_ssmc_meta(
                &hasher,
                schema.id,
                column.id,
                &old_state,
                &encoded_writes,
                prepared_column.is_touched(),
            ));
        }
    }
    let (old_state_root, new_state_root) =
        compute_state_roots_from_metas(&PoseidonHasher::new(), &metas)
            .expect("compute state roots");

    StarkTraceHarness {
        program,
        batch: Batch { transactions },
        result,
        column_metas: metas,
        old_state_root,
        new_state_root,
        schemas_by_id,
    }
}

pub fn lower_program_batch_for_harness<const WIDTH: usize>(
    harness: &StarkTraceHarness,
) -> LoweringOutput {
    lower_program_batch::<WIDTH>(
        &harness.program,
        &harness.batch,
        &harness.result,
        &harness.schemas_by_id,
        &InMemoryStaticTables::new(),
        &harness.empty_columns(),
    )
    .expect("IR lowering")
}

pub fn prepare_witness_store<const WIDTH: usize>(
    harness: &StarkTraceHarness,
    lowering: &LoweringOutput,
) -> Result<WitnessStore, TabulaError> {
    SharedStoreBuilder::<PoseidonHasher, WIDTH>::new(harness.shared_store_context())
        .prepare_witness_store(lowering, PoseidonHasher::new())
}

pub fn build_trace_map<const WIDTH: usize>(
    harness: &StarkTraceHarness,
) -> Result<TraceMap, TabulaError> {
    let lowering = lower_program_batch_for_harness::<WIDTH>(harness);
    let store = prepare_witness_store::<WIDTH>(harness, &lowering)?;
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    build_all_traces(&chips, &consumers, store)
}

pub fn debug_validate_core_trace_map(trace_map: &TraceMap) -> Result<(), TabulaError> {
    let chips = tabula_chips::core_dyn_chips();
    let buses = [
        core_buses::POSEIDON_PERM,
        core_buses::RANGE_CHECK,
        core_buses::STATIC_TABLE_LOOKUP,
    ];
    debug_validate_trace_map(&chips, &buses, trace_map)
}

pub fn build_and_validate_trace_map<const WIDTH: usize>(
    harness: &StarkTraceHarness,
) -> Result<TraceMap, TabulaError> {
    let trace_map = build_trace_map::<WIDTH>(harness)?;
    debug_validate_core_trace_map(&trace_map)?;
    Ok(trace_map)
}

fn build_ssmc_meta(
    hasher: &PoseidonHasher,
    table: TableId,
    col: ColId,
    old_state: &ColumnState<PoseidonHasher>,
    writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    is_touched: bool,
) -> ColumnMeta {
    let com_old = old_state
        .proof_commitment(table, col)
        .expect("old commitment");
    let tag = old_state.scheme_tag();
    let is_empty_old = old_state.is_empty();
    let (new_state, _) = if is_touched {
        old_state
            .apply_writes(hasher, table, col, writes)
            .expect("apply writes")
    } else {
        (old_state.clone(), com_old)
    };
    let com_new = new_state
        .proof_commitment(table, col)
        .expect("new commitment");

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

fn encode_writes(
    codec: &KoalaBearCodec,
    writes: &[tabula_witness::ColumnWrite],
) -> Result<EncodedWrites, TabulaError> {
    writes
        .iter()
        .map(|write| {
            Ok((
                write.row,
                write
                    .value
                    .as_ref()
                    .map(|value| codec.encode(value))
                    .transpose()?,
            ))
        })
        .collect()
}
