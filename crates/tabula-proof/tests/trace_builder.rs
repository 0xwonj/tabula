#![cfg(feature = "stark")]

use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{
    BabyBearCodec, ColumnMeta, CommitmentStrategy, DOMAIN_COL, DOMAIN_TABLE, HybridVC,
    MockFieldHasher, NativeDigest, PoseidonHasher, SparseMerkleTree,
};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{
    Batch, CellKey, ColId, RowKey, TableId, Transaction, TxOutcome, TxTypeId, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_proof::air::SmtPathWitness;
use tabula_proof::air::SmtTablePathWitness;
use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::execution::{InstructionRecord, Opcode};
use tabula_proof::air::chips::inter_tx_order::air::InterTxOrderChip;
use tabula_proof::air::chips::poseidon::constants::poseidon2_permutation;
use tabula_proof::air::chips::state_column::air::StateColumnChip;
use tabula_proof::air::debug_check;
use tabula_proof::trace_builder::{
    AllTraceInputs, TraceBuilder, build_all_from_program, build_trace_bundle, lower_program_batch,
};
use tabula_proof::witness::WitnessGenerator;
use tabula_proof::witness::{AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute};

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<BabyBear>)>>;

fn mk_codec() -> BabyBearCodec {
    BabyBearCodec
}

fn encode_u64(v: u64) -> Vec<BabyBear> {
    mk_codec().encode(&Value::U64(v)).expect("encode")
}

fn single_column_roots(
    vc: &HybridVC<MockFieldHasher>,
    table: TableId,
    col: ColId,
    com_old: NativeDigest,
    com_new: NativeDigest,
) -> (NativeDigest, NativeDigest) {
    let old_leaf = vc.compute_leaf(table, col, CommitmentStrategy::Ssmc, &com_old);
    let new_leaf = vc.compute_leaf(table, col, CommitmentStrategy::Ssmc, &com_new);

    let mut old_cols = BTreeMap::new();
    old_cols.insert(col, old_leaf);
    let mut new_cols = BTreeMap::new();
    new_cols.insert(col, new_leaf);

    let old_table = vc.compute_table_root(&old_cols);
    let new_table = vc.compute_table_root(&new_cols);

    let mut old_tables = BTreeMap::new();
    old_tables.insert(table, old_table);
    let mut new_tables = BTreeMap::new();
    new_tables.insert(table, new_table);

    (
        vc.compute_state_root(&old_tables),
        vc.compute_state_root(&new_tables),
    )
}

fn chain_commit_single(table: u32, col: u16, key: u64, value: &[BabyBear]) -> NativeDigest {
    const MASK_30: u64 = (1u64 << 30) - 1;
    let mut input = [BabyBear::ZERO; 16];
    input[1] = BabyBear::new(table);
    input[2] = BabyBear::new(col as u32);
    input[3] = BabyBear::new((key & MASK_30) as u32);
    input[4] = BabyBear::new(((key >> 30) & MASK_30) as u32);
    input[5] = BabyBear::new((key >> 60) as u32);
    for (i, v) in value.iter().enumerate().take(3) {
        input[6 + i] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

fn poseidon_compress(left: &NativeDigest, right: &NativeDigest) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[..8].copy_from_slice(&left.0);
    perm_input[8..16].copy_from_slice(&right.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

fn compute_leaf_digest(table: u32, col: u16, tag: u32, com: &NativeDigest) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(0x10);
    perm_input[1] = BabyBear::new(table);
    perm_input[2] = BabyBear::new(col as u32);
    perm_input[3] = BabyBear::new(tag);
    perm_input[8..16].copy_from_slice(&com.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

fn zero_siblings(depth: usize) -> Vec<NativeDigest> {
    vec![NativeDigest::ZERO; depth]
}

fn path_bits_from_key(key: u64, depth: usize) -> Vec<bool> {
    (0..depth).map(|i| ((key >> i) & 1) == 1).collect()
}

fn build_smt_paths_from_metas(
    metas: &[ColumnMeta],
    old_root: &NativeDigest,
    new_root: &NativeDigest,
) -> (Vec<SmtPathWitness>, Vec<SmtTablePathWitness>) {
    const COL_DEPTH: usize = 16;
    const TABLE_DEPTH: usize = 30;

    let hasher = PoseidonHasher::new();

    let mut by_table: BTreeMap<TableId, Vec<&ColumnMeta>> = BTreeMap::new();
    for meta in metas {
        by_table.entry(meta.table).or_default().push(meta);
    }

    let mut col_paths = Vec::new();
    let mut old_table_roots = BTreeMap::new();
    let mut new_table_roots = BTreeMap::new();
    let mut root_mults = BTreeMap::new();

    for (table, metas_for_table) in &by_table {
        let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);
        let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);

        for meta in metas_for_table {
            let old_leaf = compute_leaf_digest(
                meta.table.0,
                meta.col.0,
                match meta.tag {
                    CommitmentStrategy::Ssmc => 0,
                    CommitmentStrategy::Smt => 1,
                },
                &meta.com_old,
            );
            let new_leaf = compute_leaf_digest(
                meta.table.0,
                meta.col.0,
                match meta.tag {
                    CommitmentStrategy::Ssmc => 0,
                    CommitmentStrategy::Smt => 1,
                },
                &meta.com_new,
            );

            old_tree.insert(meta.col.0 as u64, old_leaf);
            new_tree.insert(meta.col.0 as u64, new_leaf);
        }

        for meta in metas_for_table {
            let old_leaf = compute_leaf_digest(
                meta.table.0,
                meta.col.0,
                match meta.tag {
                    CommitmentStrategy::Ssmc => 0,
                    CommitmentStrategy::Smt => 1,
                },
                &meta.com_old,
            );
            let new_leaf = compute_leaf_digest(
                meta.table.0,
                meta.col.0,
                match meta.tag {
                    CommitmentStrategy::Ssmc => 0,
                    CommitmentStrategy::Smt => 1,
                },
                &meta.com_new,
            );

            let old_proof = old_tree.prove(meta.col.0 as u64);
            let new_proof = new_tree.prove(meta.col.0 as u64);
            assert_eq!(
                old_proof.siblings, new_proof.siblings,
                "old/new sibling vectors must match for SmtColPath witness"
            );
            col_paths.push(SmtPathWitness {
                table_id: table.0,
                key: meta.col.0 as u32,
                old_leaf,
                new_leaf,
                siblings: old_proof.siblings,
                path_bits: path_bits_from_key(meta.col.0 as u64, COL_DEPTH),
            });
        }

        old_table_roots.insert(*table, old_tree.root());
        new_table_roots.insert(*table, new_tree.root());
        root_mults.insert(*table, metas_for_table.len() as u32);
    }

    let mut old_state_tree = SparseMerkleTree::new(hasher.clone(), TABLE_DEPTH, DOMAIN_TABLE);
    let mut new_state_tree = SparseMerkleTree::new(hasher, TABLE_DEPTH, DOMAIN_TABLE);
    for (&table, &root) in &old_table_roots {
        old_state_tree.insert(table.0 as u64, root);
    }
    for (&table, &root) in &new_table_roots {
        new_state_tree.insert(table.0 as u64, root);
    }

    assert_eq!(
        old_state_tree.root(),
        *old_root,
        "constructed old state root must match witness root"
    );
    assert_eq!(
        new_state_tree.root(),
        *new_root,
        "constructed new state root must match witness root"
    );

    let mut table_paths = Vec::new();
    for (&table, &root_mult) in &root_mults {
        let old_leaf = old_table_roots[&table];
        let new_leaf = new_table_roots[&table];
        let old_proof = old_state_tree.prove(table.0 as u64);
        let new_proof = new_state_tree.prove(table.0 as u64);
        assert_eq!(
            old_proof.siblings, new_proof.siblings,
            "old/new sibling vectors must match for SmtTablePath witness"
        );
        table_paths.push(SmtTablePathWitness {
            path: SmtPathWitness {
                table_id: table.0,
                key: table.0,
                old_leaf,
                new_leaf,
                siblings: old_proof.siblings,
                path_bits: path_bits_from_key(table.0 as u64, TABLE_DEPTH),
            },
            root_mult,
        });
    }

    (col_paths, table_paths)
}

#[test]
fn trace_builder_builds_valid_memory_traces() {
    let vc = HybridVC::new(MockFieldHasher, 1024);
    let table = TableId(1);
    let col = ColId(0);

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _runtime_com_old) = vc.commit_column(table, col, old_entries);

    let writes = vec![(RowKey(10), Some(encode_u64(50)))];
    let (new_state, _runtime_com_new, merge_trace) =
        vc.apply_column_writes(&old_state, table, col, &writes);
    let com_old = chain_commit_single(1, 0, 10, &encode_u64(50));
    let com_new = chain_commit_single(1, 0, 10, &encode_u64(50));

    let meta = ColumnMeta {
        table,
        col,
        tag: CommitmentStrategy::Ssmc,
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

    let bundle = TraceBuilder::<MockFieldHasher, 3>::new(&witness)
        .build_memory()
        .expect("trace bundle");

    debug_check(&InterTxOrderChip::<3>, &bundle.inter_tx_trace).expect("inter-tx trace valid");
    debug_check(&StateColumnChip::<3>, &bundle.state_trace).expect("state trace valid");
    debug_check(&ColumnMetaChip, &bundle.column_meta_trace).expect("column-meta trace valid");

    assert_eq!(bundle.inter_tx_rows.len(), 2); // init + tx row
    assert_eq!(bundle.state_rows.len(), 1); // one key in one column
}

#[test]
fn trace_builder_builds_and_validates_all_chip_bundle() {
    let vc = HybridVC::new(MockFieldHasher, 1024);
    let table = TableId(1);
    let col = ColId(0);

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, _runtime_com_old) = vc.commit_column(table, col, old_entries);

    let writes = vec![(RowKey(10), Some(encode_u64(50)))];
    let (new_state, _runtime_com_new, merge_trace) =
        vc.apply_column_writes(&old_state, table, col, &writes);
    let com_old = chain_commit_single(1, 0, 10, &encode_u64(50));
    let com_new = chain_commit_single(1, 0, 10, &encode_u64(50));

    let meta = ColumnMeta {
        table,
        col,
        tag: CommitmentStrategy::Ssmc,
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

    let memory_only = build_trace_bundle::<MockFieldHasher, 3>(&witness).expect("memory trace");
    assert_eq!(memory_only.state_rows.len(), 1);
    assert_eq!(memory_only.state_rows[0].old_hash_acc, com_old.0);
    assert_eq!(memory_only.state_rows[0].new_hash_acc, com_new.0);

    let execution_records = vec![
        InstructionRecord {
            opcode: Opcode::Read,
            tx_index: 0,
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
        siblings: zero_siblings(3),
        path_bits: vec![false, false, false],
    }];
    let smt_table_paths = vec![SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1,
            old_leaf: old_table_root,
            new_leaf: new_table_root,
            siblings: zero_siblings(3),
            path_bits: vec![true, false, false],
        },
        root_mult: 1,
    }];

    let builder = TraceBuilder::<MockFieldHasher, 3>::new(&witness);
    let bundle = builder
        .build_all(AllTraceInputs {
            execution_records: &execution_records,
            static_table_rows: &[],
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
        .expect("all-chip trace bundle");
    builder
        .debug_validate_all(&bundle)
        .expect("all-chip bundle must satisfy constraints and bus balances");
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
            let (state, _com) = vc.commit_column(schema.id, col.id, entries);
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

    let builder = TraceBuilder::<PoseidonHasher, 3>::new(&witness);
    let bundle = builder
        .build_all_from_execution_result(
            &execution_result,
            &schemas_by_id,
            &[],
            &smt_col_paths,
            &smt_table_paths,
        )
        .expect("all-chip trace assembly from execution result should succeed");
    builder
        .debug_validate_all(&bundle)
        .expect("DSL->execute->witness->trace bundle must satisfy all chip checks");
}

// ── E2E helpers for IR-based lowering tests ────────────────────────────────

/// Compile DSL, execute a batch, and generate a witness — shared scaffolding for
/// the IR-lowering and unified-pipeline E2E tests.
fn compile_execute_witness(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> (
    Program,
    Batch,
    tabula_core::ExecutionResult,
    BatchWitness<PoseidonHasher>,
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
    let env = BatchEnv {
        hasher: &hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

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
            let (state, _com) = vc.commit_column(schema.id, col_def.id, entries);
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

    (program, batch, result, witness, schemas_by_id)
}

fn make_tx(params: Vec<Value>) -> Transaction {
    Transaction {
        tx_type: TxTypeId(0),
        params,
        sender: [7u8; 32],
        nonce: 0,
        signature: vec![],
    }
}

/// Run IR-based lowering + full trace build + validation.
fn lower_build_validate(
    program: &Program,
    batch: &Batch,
    result: &tabula_core::ExecutionResult,
    witness: &BatchWitness<PoseidonHasher>,
    schemas: &BTreeMap<TableId, tabula_core::TableSchema>,
) {
    let empty_columns: BTreeSet<(TableId, ColId)> = witness
        .column_metas
        .iter()
        .filter(|m| m.is_empty_old)
        .map(|m| (m.table, m.col))
        .collect();
    let static_tables = InMemoryStaticTables::new();

    let lowering = lower_program_batch::<3>(
        program,
        batch,
        result,
        schemas,
        &static_tables,
        &empty_columns,
    )
    .expect("IR lowering");

    let (smt_col, smt_table) = tabula_proof::trace_builder::build_smt_paths(
        &witness.column_metas,
        &witness.old_state_root,
        &witness.new_state_root,
        PoseidonHasher::new(),
    )
    .expect("SMT paths");

    let builder = TraceBuilder::<PoseidonHasher, 3>::new(witness);
    let bundle = builder
        .build_all(AllTraceInputs {
            execution_records: &lowering.instruction_records,
            static_table_rows: &lowering.static_table_rows,
            smt_col_paths: &smt_col,
            smt_table_paths: &smt_table,
        })
        .expect("trace assembly");
    builder
        .debug_validate_all(&bundle)
        .expect("constraint + bus validation");
}

// ── IR-lowering E2E tests ──────────────────────────────────────────────────

#[test]
fn trace_builder_arith_add_sub_ir_lowering_e2e() {
    // Single-column program: read x, compute x+x, then (x+x)-x, write back.
    // Exercises: Read, Add, Sub, Write — all operands are Slot references.
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        vec![make_tx(vec![Value::U64(10)])],
    );
    assert!(matches!(result.tx_outcomes[0], TxOutcome::Success));
    lower_build_validate(&program, &batch, &result, &witness, &schemas);
}

#[test]
fn trace_builder_cmp_assert_ir_lowering_e2e() {
    // Single-column program: read x, compute x+x, assert x+x >= x, write (x+x)-x.
    // Exercises: Read, Add, Cmp(Gte), Assert, Sub, Write.
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    assert y >= x
    t[id].val = y - x
}";
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[(TableId(0), ColId(0), RowKey(5), Value::U64(100))],
        vec![make_tx(vec![Value::U64(5)])],
    );
    assert!(matches!(result.tx_outcomes[0], TxOutcome::Success));
    lower_build_validate(&program, &batch, &result, &witness, &schemas);
}

#[test]
fn trace_builder_full_pipeline_e2e() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
    assert!(matches!(result.tx_outcomes[0], TxOutcome::Success));

    let bundle = build_all_from_program::<PoseidonHasher, 3>(
        &witness,
        &program,
        &batch,
        &result,
        &schemas,
        &InMemoryStaticTables::new(),
        PoseidonHasher::new(),
    )
    .expect("unified pipeline");
    TraceBuilder::<PoseidonHasher, 3>::new(&witness)
        .debug_validate_all(&bundle)
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
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    let empty_columns: std::collections::BTreeSet<(TableId, ColId)> = witness
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
            "  rec[{i}]: opcode={:?} tx={} written_slots={:?} src1_idx={:?} src2_idx={:?} dst_val={:?}",
            rec.opcode, rec.tx_index, rec.written_slots, rec.src1_slot_idx, rec.src2_slot_idx,
            rec.dst_val.iter().map(|f: &BabyBear| f.as_canonical_u32()).collect::<Vec<_>>()
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
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    assert!(matches!(result.tx_outcomes[0], TxOutcome::Success));
    lower_build_validate(&program, &batch, &result, &witness, &schemas);
}
