use super::*;

// -- E2E helpers for IR-based lowering tests ----

/// Compile DSL, execute a batch, and generate a witness -- shared scaffolding for
/// the IR-lowering and unified-pipeline E2E tests.
pub(super) fn compile_execute_witness(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
) -> (
    Program,
    Batch,
    tabula_core::BatchResult,
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
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &static_tables,
        precompiles: None,
        committed_state: None,
        property_openings: None,
    };
    let result = execute_batch(&batch, &program, &snapshot, &env, &BTreeMap::new())
        .expect("batch execution");

    let commit_hasher = PoseidonHasher::new();
    let codec = KoalaBearCodec;

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
            let (state, _com) = ColumnState::commit(
                &commit_hasher,
                schema.id,
                col_def.id,
                entries,
                scheme_tags::SSMC,
            )
            .unwrap();
            old_column_states.insert((schema.id, col_def.id), state);
        }
    }

    let schemas_by_id: BTreeMap<TableId, tabula_core::TableSchema> = compiled
        .schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    let wg = WitnessGenerator::new(PoseidonHasher::new());
    let witness = wg
        .generate(&result, &schemas_by_id, &old_column_states)
        .expect("witness generation");

    (program, batch, result, witness, schemas_by_id)
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

/// Run IR-based lowering + full trace build + validation.
pub(super) fn lower_build_validate(
    program: &Program,
    batch: &Batch,
    result: &tabula_core::BatchResult,
    witness: &BatchWitness<PoseidonHasher>,
    schemas: &BTreeMap<TableId, tabula_core::TableSchema>,
) {
    let static_tables = InMemoryStaticTables::new();

    let builder = TraceBuilder::<PoseidonHasher, 3>::new(witness);
    let store = builder
        .prepare_witness_store(
            program,
            batch,
            result,
            schemas,
            &static_tables,
            PoseidonHasher::new(),
        )
        .expect("witness store preparation");

    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map =
        tabula_witness::trace::build_all_traces(&chips, &consumers, store).expect("trace assembly");

    // Only validate intra-tier buses. Cross-tier buses (ReadAccess, WriteAccess,
    // etc.) balance across execution+column+root tiers in the sharded architecture.
    let intra_tier_buses = vec![
        tabula_stark::air::interaction::core_buses::POSEIDON_PERM,
        tabula_stark::air::interaction::core_buses::RANGE_CHECK,
        tabula_stark::air::interaction::core_buses::STATIC_TABLE_LOOKUP,
    ];
    tabula_witness::trace::debug_validate_trace_map(&chips, &intra_tier_buses, &trace_map)
        .expect("constraint + bus validation");
}

// -- IR-lowering E2E tests --

#[test]
fn trace_builder_arith_add_sub_ir_lowering_e2e() {
    // Single-column program: read x, compute x+x, then (x+x)-x, write back.
    // Exercises: Read, Add, Sub, Write -- all operands are Slot references.
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
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
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
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
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
    assert!(matches!(result.txs[0], TxResult::Success { .. }));

    let builder = TraceBuilder::<PoseidonHasher, 3>::new(&witness);
    let store = builder
        .prepare_witness_store(
            &program,
            &batch,
            &result,
            &schemas,
            &InMemoryStaticTables::new(),
            PoseidonHasher::new(),
        )
        .expect("unified pipeline");

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
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    let empty_columns: BTreeSet<(TableId, ColId)> = witness
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
    let (program, batch, result, witness, schemas) = compile_execute_witness(
        source,
        &[
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        vec![make_tx(vec![Value::U64(0), Value::U64(1), Value::U64(300)])],
    );
    assert!(matches!(result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&program, &batch, &result, &witness, &schemas);
}

/// Reproduces the exact daemon web IDE scenario: 3 accounts, 3 txs, with emit.
/// This is the `transfer_example_bundle()` from tabula-driver.
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
    let (program, batch, result, witness, schemas) = compile_execute_witness(
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
    lower_build_validate(&program, &batch, &result, &witness, &schemas);
}
