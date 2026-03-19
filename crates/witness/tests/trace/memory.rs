use super::*;

fn validate_intra_tier(trace_map: &tabula_stark::trace::TraceMap) {
    let chips = tabula_chips::core_dyn_chips();
    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, trace_map)
        .expect("trace map must satisfy intra-tier checks");
}

fn single_column_context() -> TraceProofContext {
    let hasher = PoseidonHasher::new();
    let table = TableId(1);
    let col = ColId(0);
    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _) =
        ColumnState::commit(&hasher, table, col, old_entries, scheme_tags::SSMC).unwrap();
    let meta = build_ssmc_meta(
        &hasher,
        table,
        col,
        &old_state,
        &[(RowKey(10), Some(encode_u64(50)))],
        true,
    );
    let (old_state_root, new_state_root) = roots_from_metas(std::slice::from_ref(&meta));
    TraceProofContext {
        column_metas: vec![meta],
        old_state_root,
        new_state_root,
    }
}

#[test]
fn trace_builder_builds_valid_memory_traces() {
    let context = single_column_context();
    let (smt_col_paths, smt_table_paths) = build_smt_paths_from_metas(
        &context.column_metas,
        &context.old_state_root,
        &context.new_state_root,
    );

    let builder = BuiltinTraceBuilder::<MockFieldHasher, 3>::new(BuiltinTraceContext {
        column_metas: &context.column_metas,
        old_state_root: &context.old_state_root,
        new_state_root: &context.new_state_root,
    });
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &[],
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("witness store")
        .store;
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map =
        tabula_witness::trace::build_all_traces(&chips, &consumers, store).expect("trace bundle");

    assert!(!trace_map.chip_ids().is_empty());
}

#[test]
fn trace_builder_builds_and_validates_all_chip_bundle() {
    let context = single_column_context();

    let execution_records = vec![
        InstructionRecord {
            opcode: Opcode::Read,
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            written_slots: vec![0],
            src1_val: vec![KoalaBear::ZERO; 3],
            src2_val: vec![KoalaBear::ZERO; 3],
            cond_val: false,
            src1_slot_idx: None,
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: Some(1),
            access_c: Some(0),
            access_r: Some(10),
            access_val: Some(encode_u64(50)),
            access_is_null: Some(false),
            writes: vec![(0, encode_u64(50), false)],
            hash_perm_input: None,
            hash_perm_output: None,
            is_empty_col: false,
            precompile_id: None,
            property_query_type: None,
            property_query_arg0: vec![],
            property_query_arg1: vec![],
            property_result_val: vec![],
            property_result_key: vec![],
            property_result_is_null: false,
        },
        InstructionRecord {
            opcode: Opcode::Write,
            tx_index: 0,
            effect_ordinal_in_tx: 1,
            written_slots: vec![],
            src1_val: encode_u64(50),
            src2_val: vec![KoalaBear::ZERO; 3],
            cond_val: false,
            src1_slot_idx: Some(0),
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: Some(1),
            access_c: Some(0),
            access_r: Some(10),
            access_val: Some(encode_u64(50)),
            access_is_null: Some(false),
            writes: vec![],
            hash_perm_input: None,
            hash_perm_output: None,
            is_empty_col: false,
            precompile_id: None,
            property_query_type: None,
            property_query_arg0: vec![],
            property_query_arg1: vec![],
            property_result_val: vec![],
            property_result_key: vec![],
            property_result_is_null: false,
        },
    ];

    let (smt_col_paths, smt_table_paths) = build_smt_paths_from_metas(
        &context.column_metas,
        &context.old_state_root,
        &context.new_state_root,
    );

    let builder = BuiltinTraceBuilder::<MockFieldHasher, 3>::new(BuiltinTraceContext {
        column_metas: &context.column_metas,
        old_state_root: &context.old_state_root,
        new_state_root: &context.new_state_root,
    });
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &execution_records,
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("witness store")
        .store;
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = tabula_witness::trace::build_all_traces(&chips, &consumers, store)
        .expect("all-chip trace map");

    validate_intra_tier(&trace_map);
}

#[test]
fn trace_builder_dsl_execute_all_chip_e2e() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let compiled = compile(source).expect("DSL source should compile for e2e test");

    let mut program = Program::new();
    for schema in &compiled.schemas {
        program.add_schema(schema.clone());
    }
    for tx in &compiled.tx_types {
        program
            .register(tx.clone())
            .expect("compiled tx must register");
    }

    let batch = Batch {
        transactions: vec![Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(10)],
            sender: [7u8; 32],
            nonce: 0,
            signature: vec![],
        }],
    };

    let mut snapshot = InMemoryState::new();
    snapshot.set(
        CellKey {
            table: TableId(0),
            col: ColId(0),
            row: RowKey(10),
        },
        Value::U64(50),
    );

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
    let execution_result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution should succeed");

    let schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema> = compiled
        .schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    let prepared = ExecutionInputPreparer::new(PoseidonHasher::new())
        .prepare_execution_inputs(
            &execution_result,
            &schemas_by_id,
            [(TableId(0), ColId(0))].iter(),
        )
        .expect("prepared inputs");
    assert_eq!(
        prepared.access_rows_by_col[&(TableId(0), ColId(0))].len(),
        2
    );
    assert_eq!(
        prepared.access_rows_by_col[&(TableId(0), ColId(0))][0].effect_ordinal_in_tx,
        0
    );
    assert_eq!(
        prepared.access_rows_by_col[&(TableId(0), ColId(0))][1].effect_ordinal_in_tx,
        1
    );

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _) = ColumnState::commit(
        &PoseidonHasher::new(),
        TableId(0),
        ColId(0),
        old_entries,
        scheme_tags::SSMC,
    )
    .unwrap();
    let writes = prepared.writes_by_col[&(TableId(0), ColId(0))].clone();
    let meta = build_ssmc_meta(
        &PoseidonHasher::new(),
        TableId(0),
        ColId(0),
        &old_state,
        &writes,
        true,
    );
    let (old_state_root, new_state_root) = roots_from_metas(std::slice::from_ref(&meta));
    let (smt_col_paths, smt_table_paths) = build_smt_paths_from_metas(
        std::slice::from_ref(&meta),
        &old_state_root,
        &new_state_root,
    );

    let execution_records = lower_execution_records::<3>(&execution_result, &schemas_by_id)
        .expect("execution record lowering");

    let builder = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
        column_metas: std::slice::from_ref(&meta),
        old_state_root: &old_state_root,
        new_state_root: &new_state_root,
    });
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &execution_records,
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("witness store")
        .store;
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = tabula_witness::trace::build_all_traces(&chips, &consumers, store)
        .expect("all-chip trace assembly from execution result should succeed");

    validate_intra_tier(&trace_map);
}
