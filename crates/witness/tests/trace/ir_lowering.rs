use super::*;

/// Compile DSL, execute a batch, and derive the builtin trace context from the
/// new execution-input preparation path.
pub(super) fn compile_execute_context(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> (
    Program,
    Batch,
    tabula_core::BatchResult,
    TraceProofContext,
    BTreeMap<TableId, tabula_core::TableSchema>,
) {
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

    let batch = Batch { transactions };
    let hasher = PoseidonHasher::new();
    let static_tables = InMemoryStaticTables::new();
    let property_queries = tabula_executor::property::PropertyQueryRegistry::new();
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
                &PoseidonHasher::new(),
                schema.id,
                col_def.id,
                &old_state,
                &writes,
                prepared.touched.contains(&(schema.id, col_def.id)),
            ));
        }
    }
    let (old_state_root, new_state_root) = preparer.compute_state_roots_from_metas(&metas);

    (
        program,
        batch,
        result,
        TraceProofContext {
            column_metas: metas,
            old_state_root,
            new_state_root,
        },
        schemas_by_id,
    )
}

pub(super) fn make_tx(params: Vec<Value>) -> Transaction {
    Transaction {
        tx_type: TxTypeId(0),
        params,
        sender: [7u8; 32],
        nonce: 0,
        signature: vec![],
    }
}

fn lower_for_context(
    program: &Program,
    batch: &Batch,
    result: &tabula_core::BatchResult,
    context: &TraceProofContext,
    schemas: &BTreeMap<TableId, tabula_core::TableSchema>,
) -> tabula_witness::trace::builtin::lowering::LoweringOutput {
    let empty_columns: BTreeSet<(TableId, ColId)> = context
        .column_metas
        .iter()
        .filter(|meta| meta.is_empty_old)
        .map(|meta| (meta.table, meta.col))
        .collect();

    lower_program_batch::<3>(
        program,
        batch,
        result,
        schemas,
        &InMemoryStaticTables::new(),
        &empty_columns,
    )
    .expect("IR lowering")
}

/// Run IR-based lowering + full trace build + validation.
pub(super) fn lower_build_validate(
    program: &Program,
    batch: &Batch,
    result: &tabula_core::BatchResult,
    context: &TraceProofContext,
    schemas: &BTreeMap<TableId, tabula_core::TableSchema>,
) {
    let lowering = lower_for_context(program, batch, result, context, schemas);

    let builder = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
        column_metas: &context.column_metas,
        old_state_root: &context.old_state_root,
        new_state_root: &context.new_state_root,
    });
    let store = builder
        .prepare_witness_store(&lowering, PoseidonHasher::new())
        .expect("witness store preparation")
        .store;

    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map =
        tabula_witness::trace::build_all_traces(&chips, &consumers, store).expect("trace assembly");

    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, &trace_map)
        .expect("constraint + bus validation");
}

#[test]
fn trace_builder_arith_add_sub_ir_lowering_e2e() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        vec![make_tx(vec![Value::U64(10)])],
    );
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&program, &batch, &result, &context, &schemas);
}

#[test]
fn trace_builder_cmp_assert_ir_lowering_e2e() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    assert y >= x
    t[id].val = y - x
}";
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[(TableId(0), ColId(0), RowKey(5), Value::U64(100))],
        vec![make_tx(vec![Value::U64(5)])],
    );
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&program, &batch, &result, &context, &schemas);
}

#[test]
fn trace_builder_full_pipeline_e2e() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
    assert!(matches!(result.txs[0], TxResult::Success { .. }));

    let builder = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
        column_metas: &context.column_metas,
        old_state_root: &context.old_state_root,
        new_state_root: &context.new_state_root,
    });
    let lowering = lower_for_context(&program, &batch, &result, &context, &schemas);
    let store = builder
        .prepare_witness_store(&lowering, PoseidonHasher::new())
        .expect("unified pipeline")
        .store;

    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map =
        tabula_witness::trace::build_all_traces(&chips, &consumers, store).expect("trace assembly");

    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, &trace_map)
        .expect("unified pipeline must satisfy all constraints");
}

#[test]
fn trace_builder_transfer_param_debug_lowering() {
    use p3_field::PrimeField32;

    let source = "\
table balances { balance: u64 }
tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
}";
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    let empty_columns: BTreeSet<(TableId, ColId)> = context
        .column_metas
        .iter()
        .filter(|m| m.is_empty_old)
        .map(|m| (m.table, m.col))
        .collect();
    let static_tables = InMemoryStaticTables::new();
    let lowering = lower_program_batch::<3>(
        &program,
        &batch,
        &result,
        &schemas,
        &static_tables,
        &empty_columns,
    )
    .expect("IR lowering");
    for (i, rec) in lowering.instruction_records.iter().enumerate() {
        eprintln!(
            "  rec[{i}]: opcode={:?} tx={} written_slots={:?} src1_idx={:?} src2_idx={:?} writes={:?}",
            rec.opcode,
            rec.tx_index,
            rec.written_slots,
            rec.src1_slot_idx,
            rec.src2_slot_idx,
            rec.writes
                .iter()
                .map(|(s, v, n)| (
                    s,
                    v.iter()
                        .map(|f: &KoalaBear| f.as_canonical_u32())
                        .collect::<Vec<_>>(),
                    n
                ))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn trace_builder_transfer_param_materialization_e2e() {
    let source = "\
table balances { balance: u64 }
tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
}";
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&program, &batch, &result, &context, &schemas);
}

#[test]
fn trace_builder_transfer_3tx_with_emit_e2e() {
    let source = "\
table balances { balance: u64 }
tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
    emit \"transfer\" (from, to, amount)
}";
    let sender = [1u8; 32];
    let (program, batch, result, context, schemas) = compile_execute_context(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
            (TableId(0), ColId(0), RowKey(2), Value::U64(200)),
        ],
        vec![
            Transaction {
                tx_type: TxTypeId(0),
                params: vec![Value::U64(0), Value::U64(1), Value::U64(300)],
                sender,
                nonce: 0,
                signature: vec![],
            },
            Transaction {
                tx_type: TxTypeId(0),
                params: vec![Value::U64(1), Value::U64(2), Value::U64(200)],
                sender,
                nonce: 1,
                signature: vec![],
            },
            Transaction {
                tx_type: TxTypeId(0),
                params: vec![Value::U64(2), Value::U64(0), Value::U64(50)],
                sender,
                nonce: 2,
                signature: vec![],
            },
        ],
    );
    for (i, outcome) in result.txs.iter().enumerate() {
        assert!(
            matches!(outcome, TxResult::Success { .. }),
            "tx {i} should succeed, got: {outcome:?}"
        );
    }
    lower_build_validate(&program, &batch, &result, &context, &schemas);
}
