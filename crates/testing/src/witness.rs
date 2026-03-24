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
    Batch, ColId, InMemoryStaticTables, PortableValue, RowKey, TableId, TableSchema, Transaction,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{ExecutionJournal, ResolvedExecutionProgram, derive_batch_report};
use tabula_ir::Program;
use tabula_stark::air::interaction::core_buses;
use tabula_stark::trace::{TraceMap, WitnessStore, build_all_traces, debug_validate_trace_map};
use tabula_types::{EncodingRuntime, EncodingRuntimeRegistry, TypeRuntimeRegistry};
use tabula_witness::stark::{
    LowerSuccessfulTxInput, LoweringOutput, LoweringPrecompileCall, LoweringPropertyRead,
    SmtRootStoreContext, TxLoweringOutput, lower_successful_tx, prepare_execution_store,
    prepare_smt_root_store,
};
use tabula_witness::{AccessEvent, ColumnWrite};

use tabula_chips::ir_hash::{IR_HASH_BUS, IR_HASH_CHIP_ID, IrHashChip};

type PortableColumnEntry = (RowKey, PortableValue);
type PortableColumnEntries = BTreeMap<(TableId, ColId), Vec<PortableColumnEntry>>;
type EncodedWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Shared STARK witness/trace harness used by tests, benches, and E2E checks.
pub struct StarkTraceHarness {
    pub program: Program,
    pub batch: Batch,
    pub execution_journal: ExecutionJournal,
    pub result: tabula_core::BatchReport,
    pub column_root_bindings: Vec<ColumnRootBinding>,
    pub old_state_root: NativeDigest,
    pub new_state_root: NativeDigest,
    pub schemas_by_id: BTreeMap<TableId, TableSchema>,
    pub type_runtimes: TypeRuntimeRegistry,
    pub encoding_runtimes: EncodingRuntimeRegistry,
}

impl StarkTraceHarness {
    pub fn smt_root_store_context(&self) -> SmtRootStoreContext<'_> {
        SmtRootStoreContext::new(
            &self.column_root_bindings,
            &self.old_state_root,
            &self.new_state_root,
        )
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
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let resolved = ResolvedExecutionProgram::from_program(&program).expect("resolved program");
    let journal = execute_batch(&batch, &resolved, &snapshot, &env).expect("journal execution");
    let result = derive_batch_report(&journal, &type_runtimes).expect("batch result projection");

    let schemas_by_id: BTreeMap<TableId, TableSchema> = compiled_schemas
        .iter()
        .cloned()
        .map(|schema| (schema.id, schema))
        .collect();
    let writes_by_column = group_column_writes(&journal);

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
            let writes = writes_by_column
                .get(&(schema.id, column.id))
                .cloned()
                .unwrap_or_default();
            let encoded_writes =
                encode_writes(encoding_runtime.as_ref(), &writes).expect("encode writes");
            root_bindings.push(build_ssmc_root_binding(
                &hasher,
                (schema.id, column.id),
                resolved.root_binding_family(),
                resolved.column_profile.profile_hash,
                &old_state,
                &encoded_writes,
                !writes.is_empty(),
            ));
        }
    }
    let (old_state_root, new_state_root) =
        compute_state_roots_from_bindings(&PoseidonHasher::new(), &root_bindings)
            .expect("compute state roots");

    StarkTraceHarness {
        program,
        batch: Batch { transactions },
        execution_journal: journal,
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
    let profile_map = build_profile_map(
        &harness.schemas_by_id,
        harness.program.profile_catalog(),
        &harness.type_runtimes,
        &harness.encoding_runtimes,
    )
    .expect("profile map");
    let empty_columns = harness.empty_columns();
    let mut instruction_records = Vec::new();
    let mut static_rows: BTreeMap<
        (u32, u16, u64),
        tabula_chips::static_table::trace::StaticTableRow,
    > = BTreeMap::new();
    let mut ir_hash_calls = Vec::new();

    for success in harness.execution_journal.successful_txs() {
        let tx = harness
            .batch
            .transactions
            .get(success.tx_index as usize)
            .expect("batch transaction");
        let tx_def = harness
            .program
            .resolve(tx.tx_type)
            .expect("resolved tx definition");
        let access_trace = success
            .access_effects
            .iter()
            .map(|effect| witness_access_event(success.tx_index, effect, &harness.type_runtimes))
            .collect::<Result<Vec<_>, _>>()
            .expect("access trace");
        let precompile_calls = success
            .precompile_calls
            .iter()
            .map(|effect| LoweringPrecompileCall {
                instruction_index: effect.instruction_index,
                precompile_id: effect.precompile_id,
                inputs: effect.inputs.clone(),
                outputs: effect.outputs.clone(),
            })
            .collect::<Vec<_>>();
        let property_reads = success
            .property_reads
            .iter()
            .map(|effect| LoweringPropertyRead {
                instruction_index: effect.instruction_index,
                result: effect.result.clone(),
            })
            .collect::<Vec<_>>();
        let lowering = lower_successful_tx::<WIDTH>(LowerSuccessfulTxInput {
            tx_index: success.tx_index,
            tx,
            tx_def,
            profile_map: &profile_map,
            type_runtimes: &harness.type_runtimes,
            encoding_runtimes: &harness.encoding_runtimes,
            static_tables: &InMemoryStaticTables::new(),
            empty_columns: &empty_columns,
            precompile_signatures: harness.program.precompiles(),
            access_trace: &access_trace,
            precompile_calls: &precompile_calls,
            property_reads: &property_reads,
        })
        .expect("lower successful tx");
        merge_tx_lowering(
            lowering,
            &mut instruction_records,
            &mut static_rows,
            &mut ir_hash_calls,
        );
    }

    LoweringOutput {
        instruction_records,
        static_table_rows: static_rows.into_values().collect(),
        ir_hash_calls,
    }
}

pub fn prepare_witness_store<const WIDTH: usize>(
    harness: &StarkTraceHarness,
    lowering: &LoweringOutput,
) -> Result<WitnessStore, TabulaError> {
    let mut store = prepare_execution_store(lowering)?;
    store
        .merge(prepare_smt_root_store(
            harness.smt_root_store_context(),
            PoseidonHasher::new(),
        )?)
        .map_err(|detail| TabulaError::ProofError {
            phase: "store_merge",
            detail,
        })?;
    Ok(store)
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

fn group_column_writes(journal: &ExecutionJournal) -> BTreeMap<(TableId, ColId), Vec<ColumnWrite>> {
    let mut grouped = BTreeMap::new();
    for entry in &journal.state_summary.write_set_final {
        grouped
            .entry((entry.key.table, entry.key.col))
            .or_insert_with(Vec::new)
            .push(ColumnWrite {
                row: entry.key.row,
                value: entry.value.clone(),
            });
    }
    for writes in grouped.values_mut() {
        writes.sort_by_key(|write| write.row);
    }
    grouped
}

fn build_profile_map(
    schemas: &BTreeMap<TableId, TableSchema>,
    profile_catalog: &tabula_profile::ProfileCatalog,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<BTreeMap<(TableId, ColId), tabula_witness::ColumnValueProfile>, TabulaError> {
    let mut profile_map = BTreeMap::new();
    for (&table_id, schema) in schemas {
        for col in &schema.columns {
            let resolved = profile_catalog
                .resolve_column_profile(col.column_profile_id)
                .map_err(|err| TabulaError::ProofError {
                    phase: "testing_witness",
                    detail: format!(
                        "column profile {} for table {} col {} is invalid: {err}",
                        col.column_profile_id.0, table_id.0, col.id.0,
                    ),
                })?;
            type_runtimes.resolve(resolved.type_descriptor.type_id)?;
            encoding_runtimes.resolve(resolved.encoding_profile.encoding_profile_id)?;
            profile_map.insert(
                (table_id, col.id),
                tabula_witness::ColumnValueProfile {
                    type_id: resolved.type_descriptor.type_id,
                    encoding_profile_id: resolved.encoding_profile.encoding_profile_id,
                },
            );
        }
    }
    Ok(profile_map)
}

fn witness_access_event(
    tx_index: u32,
    effect: &tabula_executor::TypedAccessEffect,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<AccessEvent, TabulaError> {
    let value = match &effect.value {
        Some(value) => value.clone(),
        None => type_runtimes.zero_of(effect.type_id)?,
    };
    Ok(AccessEvent {
        key: effect.key,
        time: effect.logical_time,
        is_write: effect.op == tabula_core::OpKind::Write,
        value,
        is_null: effect.value.is_none(),
        tx_index,
        effect_ordinal_in_tx: effect.effect_ordinal_in_tx,
    })
}

fn merge_tx_lowering(
    lowering: TxLoweringOutput,
    instruction_records: &mut Vec<tabula_chips::execution::trace::InstructionRecord>,
    static_rows: &mut BTreeMap<(u32, u16, u64), tabula_chips::static_table::trace::StaticTableRow>,
    ir_hash_calls: &mut Vec<tabula_chips::ir_hash::IrHashCall>,
) {
    instruction_records.extend(lowering.instruction_records);
    ir_hash_calls.extend(lowering.ir_hash_calls);
    for row in lowering.static_table_rows {
        let key = (row.table_id, row.col_id, row.row_key);
        static_rows
            .entry(key)
            .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
            .or_insert(row);
    }
}
