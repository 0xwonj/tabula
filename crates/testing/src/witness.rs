use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use crate::exec::{in_memory_state_from_cells, program_from_source};
use crate::fixtures::cases::TraceCase;

use tabula_commitment::schemes::ssmc::{SsmcEntry, SsmcList};
use tabula_commitment::{
    ColumnRootBinding, NativeDigest, NormalizedVerifierDigest, PoseidonHasher,
    compute_column_root_binding_prefix_digest, compute_state_roots_from_bindings,
};
use tabula_core::error::TabulaError;
use tabula_core::{
    Batch, ColId, InMemoryStaticTables, NoopSigVerifier, PortableValue, RowKey, SequentialNonce,
    TableId, TableSchema, Transaction,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_stark::air::interaction::core_buses;
use tabula_stark::trace::{TraceMap, WitnessStore, build_all_traces, debug_validate_trace_map};
use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypeRuntimeRegistry};
use tabula_witness::stark::{
    LowerProgramBatchInput, LoweringOutput, SharedStoreBuilder, SharedStoreContext,
    lower_program_batch,
};
use tabula_witness::{ExecutionInputPreparer, PreparedExecutionColumns};

use tabula_chips::ir_hash::{IR_HASH_BUS, IR_HASH_CHIP_ID, IrHashChip};

type PortableColumnEntry = (RowKey, PortableValue);
type PortableColumnEntries = BTreeMap<(TableId, ColId), Vec<PortableColumnEntry>>;
type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Shared STARK witness/trace harness used by tests, benches, and E2E checks.
pub struct StarkTraceHarness {
    pub program: Program,
    pub batch: Batch,
    pub result: tabula_core::BatchResult,
    pub column_root_bindings: Vec<ColumnRootBinding>,
    pub old_state_root: NativeDigest,
    pub new_state_root: NativeDigest,
    pub schemas_by_id: BTreeMap<TableId, TableSchema>,
    pub type_runtimes: TypeRuntimeRegistry,
    pub encoding_runtimes: EncodingRuntimeRegistry,
}

impl StarkTraceHarness {
    pub fn shared_store_context(&self) -> SharedStoreContext<'_> {
        SharedStoreContext {
            column_root_bindings: &self.column_root_bindings,
            old_state_root: &self.old_state_root,
            new_state_root: &self.new_state_root,
        }
    }

    pub fn empty_columns(&self) -> BTreeSet<(TableId, ColId)> {
        self.column_root_bindings
            .iter()
            .filter(|binding| binding.is_empty_old)
            .map(|binding| (binding.table, binding.col))
            .collect()
    }
}

pub fn compile_execute_case(case: &TraceCase) -> StarkTraceHarness {
    compile_execute_context_impl(case.source, &case.initial_cells, case.transactions.clone())
}

pub fn compile_execute_context(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, PortableValue)],
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
        harness.program.profile_catalog(),
        &harness.type_runtimes,
        &harness.encoding_runtimes,
        planned_columns.iter(),
    )
}

fn compile_execute_context_impl(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, PortableValue)],
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
    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
    let encoding_runtimes = EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
    let env = BatchEnv {
        hasher: &hasher,
        type_runtimes: &type_runtimes,
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
        .prepare_execution_inputs(
            &result,
            &schemas_by_id,
            program.profile_catalog(),
            &type_runtimes,
            &encoding_runtimes,
            planned_columns.iter(),
        )
        .expect("prepared execution inputs");

    let mut entries_by_col: PortableColumnEntries = BTreeMap::new();
    for (table, col, row, value) in initial_cells {
        entries_by_col
            .entry((*table, *col))
            .or_default()
            .push((*row, value.clone()));
    }

    let mut root_bindings = Vec::new();
    let hasher = PoseidonHasher::new();
    for schema in &compiled_schemas {
        for column in &schema.columns {
            let resolved = program
                .profile_catalog()
                .resolve_column_profile(column.column_profile_id)
                .expect("resolve column profile");
            let prepared_column = prepared
                .columns
                .iter()
                .find(|prepared_column| {
                    prepared_column.table == schema.id && prepared_column.col == column.id
                })
                .expect("prepared column");
            let entries = entries_by_col
                .remove(&(schema.id, column.id))
                .unwrap_or_default();
            let type_runtime = type_runtimes
                .resolve(resolved.type_descriptor.type_id)
                .expect("column type runtime");
            let encoding_runtime = encoding_runtimes
                .resolve(resolved.encoding_profile.encoding_profile_id)
                .expect("column encoding runtime");
            let mut encoded_entries = entries
                .into_iter()
                .map(|(row, portable)| {
                    let typed = type_runtime
                        .decode_portable(&portable)
                        .expect("decode initial portable cell");
                    (
                        row,
                        encoding_runtime
                            .encode_field_elements(&typed)
                            .expect("encode initial typed cell"),
                    )
                })
                .collect::<Vec<_>>();
            encoded_entries.sort_by_key(|(row, _)| *row);
            let old_state = SsmcList::from_sorted(
                schema.id,
                column.id,
                encoded_entries
                    .into_iter()
                    .map(|(key, value)| SsmcEntry { key, value })
                    .collect(),
            )
            .expect("commit old state");
            let encoded_writes = encode_writes(encoding_runtime.as_ref(), &prepared_column.writes)
                .expect("encode writes");
            root_bindings.push(build_ssmc_root_binding(
                &hasher,
                (schema.id, column.id),
                resolved.root_binding_family(),
                resolved.column_profile.profile_hash,
                &old_state,
                &encoded_writes,
                prepared_column.is_touched(),
            ));
        }
    }
    let (old_state_root, new_state_root) =
        compute_state_roots_from_bindings(&PoseidonHasher::new(), &root_bindings)
            .expect("compute state roots");

    StarkTraceHarness {
        program,
        batch: Batch { transactions },
        result,
        column_root_bindings: root_bindings,
        old_state_root,
        new_state_root,
        schemas_by_id,
        type_runtimes,
        encoding_runtimes,
    }
}

pub fn lower_program_batch_for_harness<const WIDTH: usize>(
    harness: &StarkTraceHarness,
) -> LoweringOutput {
    lower_program_batch::<WIDTH>(LowerProgramBatchInput {
        program: &harness.program,
        batch: &harness.batch,
        result: &harness.result,
        schemas: &harness.schemas_by_id,
        type_runtimes: &harness.type_runtimes,
        encoding_runtimes: &harness.encoding_runtimes,
        static_tables: &InMemoryStaticTables::new(),
        empty_columns: &harness.empty_columns(),
    })
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
    let mut chips = tabula_chips::core_dyn_chips();
    if !lowering.ir_hash_calls.is_empty() {
        chips.push(Box::new(IrHashChip));
    }
    let consumers = tabula_chips::core_bus_consumers();
    build_all_traces(&chips, &consumers, store)
}

pub fn debug_validate_core_trace_map(trace_map: &TraceMap) -> Result<(), TabulaError> {
    let mut chips = tabula_chips::core_dyn_chips();
    let mut buses = vec![
        core_buses::POSEIDON_PERM,
        core_buses::RANGE_CHECK,
        core_buses::STATIC_TABLE_LOOKUP,
    ];
    if trace_map.chip_ids().contains(&IR_HASH_CHIP_ID) {
        chips.push(Box::new(IrHashChip));
        buses.push(IR_HASH_BUS);
    }
    debug_validate_trace_map(&chips, &buses, trace_map)
}

pub fn build_and_validate_trace_map<const WIDTH: usize>(
    harness: &StarkTraceHarness,
) -> Result<TraceMap, TabulaError> {
    let trace_map = build_trace_map::<WIDTH>(harness)?;
    debug_validate_core_trace_map(&trace_map)?;
    Ok(trace_map)
}

fn build_ssmc_root_binding(
    hasher: &PoseidonHasher,
    column: (TableId, ColId),
    root_binding_family: tabula_core::RootProfileId,
    column_profile_hash: tabula_core::Digest,
    old_state: &SsmcList,
    writes: &[(RowKey, Option<Vec<KoalaBear>>)],
    is_touched: bool,
) -> ColumnRootBinding {
    let (table, col) = column;
    let com_old = old_state.proof_commitment().expect("old commitment");
    let is_empty_old = old_state.is_empty();
    let new_state = if is_touched {
        old_state.apply_writes(writes, hasher).0
    } else {
        old_state.clone()
    };
    let com_new = new_state.proof_commitment().expect("new commitment");
    let binding_digest = compute_column_root_binding_prefix_digest(
        hasher,
        table,
        col,
        root_binding_family,
        &column_profile_hash,
    );

    ColumnRootBinding {
        table,
        col,
        root_binding_family,
        column_profile_hash,
        binding_digest,
        old_digest: NormalizedVerifierDigest::new(com_old),
        new_digest: NormalizedVerifierDigest::new(com_new),
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched,
    }
}

fn encode_writes(
    encoding_runtime: &dyn EncodingRuntime,
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
                    .map(|value| encoding_runtime.encode_field_elements(value))
                    .transpose()?,
            ))
        })
        .collect()
}
