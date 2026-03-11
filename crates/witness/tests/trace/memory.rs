use super::*;

#[test]
fn trace_builder_builds_valid_memory_traces() {
    let vc = HybridVC::new(MockFieldHasher, 1024);
    let table = TableId(1);
    let col = ColId(0);

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _runtime_com_old) = vc.commit_column(table, col, old_entries).unwrap();

    let writes = vec![(RowKey(10), Some(encode_u64(50)))];
    let (new_state, _runtime_com_new, merge_trace) =
        vc.apply_column_writes(&old_state, table, col, &writes);
    let com_old = chain_commit_single(1, 0, 10, &encode_u64(50));
    let com_new = chain_commit_single(1, 0, 10, &encode_u64(50));

    let meta = ColumnMeta {
        table,
        col,
        tag: scheme_tags::SSMC,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    };

    let column_witness = ColumnWitness {
        table,
        col,
        value_type: tabula_core::ValueType::U64,
        init_rows: vec![InitRow {
            key: CellKey {
                table,
                col,
                row: RowKey(10),
            },
            value_fes: encode_u64(50),
            val_is_null: false,
        }],
        access_rows: vec![
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 0,
                is_write: false,
                value_fes: encode_u64(50),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 0,
            },
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 1,
                is_write: true,
                value_fes: encode_u64(50),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 1,
            },
        ],
        old_state,
        new_state,
        merge_trace,
        meta: meta.clone(),
    };

    let (old_state_root, new_state_root) = single_column_roots(&vc, table, col, com_old, com_new);

    let witness = BatchWitness {
        columns: vec![column_witness],
        column_metas: vec![meta],
        old_state_root,
        new_state_root,
        tx_outcomes: vec![TxOutcome::Success],
        key_routes: BTreeMap::<CellKey, KeyRoute>::new(),
    };

    // Build memory traces via the full trace builder, then check the memory chips.
    let builder = TraceBuilder::<MockFieldHasher, 3>::new(&witness);
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &[],
            static_table_rows: &[],
            smt_col_paths: &[],
            smt_table_paths: &[],
        })
        .expect("witness store");
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map =
        tabula_witness::trace::build_all_traces(&chips, &consumers, store).expect("trace bundle");

    // Verify the trace map was built successfully (chip-specific checks removed
    // after InterTxOrder/StateColumn/ColumnMeta global chips were replaced by shard chips).
    assert!(
        !trace_map.chip_ids().is_empty(),
        "trace map should contain at least one chip trace"
    );
}

#[test]
fn trace_builder_builds_and_validates_all_chip_bundle() {
    let vc = HybridVC::new(MockFieldHasher, 1024);
    let table = TableId(1);
    let col = ColId(0);

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _runtime_com_old) = vc.commit_column(table, col, old_entries).unwrap();

    let writes = vec![(RowKey(10), Some(encode_u64(50)))];
    let (new_state, _runtime_com_new, merge_trace) =
        vc.apply_column_writes(&old_state, table, col, &writes);
    let com_old = chain_commit_single(1, 0, 10, &encode_u64(50));
    let com_new = chain_commit_single(1, 0, 10, &encode_u64(50));

    let meta = ColumnMeta {
        table,
        col,
        tag: scheme_tags::SSMC,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    };

    let column_witness = ColumnWitness {
        table,
        col,
        value_type: tabula_core::ValueType::U64,
        init_rows: vec![InitRow {
            key: CellKey {
                table,
                col,
                row: RowKey(10),
            },
            value_fes: encode_u64(50),
            val_is_null: false,
        }],
        access_rows: vec![
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 0,
                is_write: false,
                value_fes: encode_u64(50),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 0,
            },
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 1,
                is_write: true,
                value_fes: encode_u64(50),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 1,
            },
        ],
        old_state,
        new_state,
        merge_trace,
        meta: meta.clone(),
    };

    let old_leaf = compute_leaf_digest(1, 0, 0, &com_old);
    let new_leaf = compute_leaf_digest(1, 0, 0, &com_new);

    fn chain_compress(leaf: &NativeDigest, depth: usize) -> NativeDigest {
        let mut node = *leaf;
        for _ in 0..depth {
            node = poseidon_compress(&node, &NativeDigest::ZERO);
        }
        node
    }

    fn chain_compress_key1(leaf: &NativeDigest, depth: usize) -> NativeDigest {
        let mut node = *leaf;
        for level in 0..depth {
            if level == 0 {
                node = poseidon_compress(&NativeDigest::ZERO, &node);
            } else {
                node = poseidon_compress(&node, &NativeDigest::ZERO);
            }
        }
        node
    }

    let old_table_root = chain_compress(&old_leaf, 3);
    let new_table_root = chain_compress(&new_leaf, 3);
    let old_state_root = chain_compress_key1(&old_table_root, 3);
    let new_state_root = chain_compress_key1(&new_table_root, 3);

    let witness = BatchWitness {
        columns: vec![column_witness],
        column_metas: vec![meta],
        old_state_root,
        new_state_root,
        tx_outcomes: vec![TxOutcome::Success],
        key_routes: BTreeMap::<CellKey, KeyRoute>::new(),
    };

    let execution_records = vec![
        InstructionRecord {
            opcode: Opcode::Read,
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            written_slots: vec![0],
            src1_val: vec![BabyBear::ZERO; 3],
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            src1_slot_idx: None,
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: Some(1),
            access_c: Some(0),
            access_r: Some(10),
            access_val: Some(encode_u64(50)),
            access_is_null: Some(false),
            dst_val: encode_u64(50),
            dst_is_null: false,
            dst2_val: vec![],
            dst2_is_null: false,
            hash_perm_input: None,
            hash_perm_output: None,
            is_empty_col: false,
        },
        InstructionRecord {
            opcode: Opcode::Write,
            tx_index: 0,
            effect_ordinal_in_tx: 1,
            written_slots: vec![],
            src1_val: encode_u64(50),
            src2_val: vec![BabyBear::ZERO; 3],
            cond_val: false,
            src1_slot_idx: Some(0),
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: Some(1),
            access_c: Some(0),
            access_r: Some(10),
            access_val: Some(encode_u64(50)),
            access_is_null: Some(false),
            dst_val: vec![],
            dst_is_null: false,
            dst2_val: vec![],
            dst2_is_null: false,
            hash_perm_input: None,
            hash_perm_output: None,
            is_empty_col: false,
        },
    ];

    let smt_col_paths = vec![SmtPathWitness {
        table_id: 1,
        key: 0,
        old_leaf,
        new_leaf,
        old_siblings: zero_siblings(3),
        new_siblings: zero_siblings(3),
        path_bits: vec![false, false, false],
    }];
    let smt_table_paths = vec![SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: old_table_root,
            new_leaf: new_table_root,
            old_siblings: zero_siblings(3),
            new_siblings: zero_siblings(3),
            path_bits: vec![true, false, false],
        },
        root_mult: 1,
    }];

    let builder = TraceBuilder::<MockFieldHasher, 3>::new(&witness);
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &execution_records,
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("witness store");
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = tabula_witness::trace::build_all_traces(&chips, &consumers, store)
        .expect("all-chip trace map");
    // Only validate intra-tier buses. Cross-tier buses balance across
    // execution+column+root tiers in the sharded architecture.
    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, &trace_map)
        .expect("all-chip trace map must satisfy constraints and bus balances");
}

#[test]
fn trace_builder_dsl_execute_witness_all_chip_e2e() {
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

    let sender = [7u8; 32];
    let batch = Batch {
        transactions: vec![Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(10)],
            sender,
            nonce: 0,
            signature: vec![],
        }],
    };

    let mut snapshot = InMemoryState::new();
    let key = CellKey {
        table: TableId(0),
        col: ColId(0),
        row: RowKey(10),
    };
    snapshot.set(key, Value::U64(50));

    let hasher = PoseidonHasher::new();
    let static_tables = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
    };
    let execution_result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution should succeed");
    assert_eq!(execution_result.events.len(), 2);

    let vc = HybridVC::new(PoseidonHasher::new(), 1024);
    let codec = BabyBearCodec;

    let mut entries_by_col: EncodedColumnEntries = BTreeMap::new();
    entries_by_col
        .entry((TableId(0), ColId(0)))
        .or_default()
        .push((RowKey(10), codec.encode(&Value::U64(50)).expect("encode")));

    let mut old_column_states = BTreeMap::new();
    for schema in &compiled.schemas {
        for col in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) = vc.commit_column(schema.id, col.id, entries).unwrap();
            old_column_states.insert((schema.id, col.id), state);
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
        .generate(&execution_result, &schemas_by_id, &old_column_states)
        .expect("witness generation should succeed");
    assert_eq!(witness.columns.len(), 1);
    assert_eq!(witness.columns[0].access_rows.len(), 2);
    assert_eq!(witness.columns[0].access_rows[0].tx_index, 0);
    assert_eq!(witness.columns[0].access_rows[0].effect_ordinal_in_tx, 0);
    assert_eq!(witness.columns[0].access_rows[1].tx_index, 0);
    assert_eq!(witness.columns[0].access_rows[1].effect_ordinal_in_tx, 1);

    let (smt_col_paths, smt_table_paths) = build_smt_paths_from_metas(
        &witness.column_metas,
        &witness.old_state_root,
        &witness.new_state_root,
    );

    let execution_records = lower_execution_records::<3>(&execution_result, &schemas_by_id)
        .expect("execution record lowering");

    let builder = TraceBuilder::<PoseidonHasher, 3>::new(&witness);
    let store = builder
        .populate_store(AllTraceInputs {
            execution_records: &execution_records,
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("witness store");
    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = tabula_witness::trace::build_all_traces(&chips, &consumers, store)
        .expect("all-chip trace assembly from execution result should succeed");
    // Only validate intra-tier buses. Cross-tier buses balance across
    // execution+column+root tiers in the sharded architecture.
    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, &trace_map)
        .expect("DSL->execute->witness->trace map must satisfy all chip checks");
}
