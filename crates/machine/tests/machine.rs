//! Tests for ChipRegistry, TabulaMachine, and MachineBuilder.

use std::collections::BTreeMap;

use tabula_chips::range_check::RangeCheckChip;
use tabula_commitment::{BabyBearCodec, HybridVC, PoseidonHasher};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, Transaction, TxTypeId, Value};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_machine::{ChipRegistry, SetupError, TabulaMachine, core_chips};
use tabula_stark::air::statement::PublicStatement;
use tabula_stark::chips::core_chips as core_chip_ids;
use tabula_witness::WitnessGenerator;
use tabula_witness::trace::build_trace_map;

// ── Builder basics ───────────────────────────────────────────────────────────

#[test]
fn builder_with_core_chips_succeeds() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("core chip registration should succeed");

    assert_eq!(machine.registry().chip_ids().len(), 9);
}

#[test]
fn empty_registry_fails() {
    let result = TabulaMachine::builder().build();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, SetupError::EmptyRegistry),
        "expected EmptyRegistry, got: {err}"
    );
}

#[test]
fn duplicate_chip_fails() {
    let result = TabulaMachine::builder()
        .with_chip(RangeCheckChip)
        .with_chip(RangeCheckChip)
        .build();

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, SetupError::DuplicateChipId(_)),
        "expected DuplicateChipId, got: {err}"
    );
}

// ── Registry contents ────────────────────────────────────────────────────────

#[test]
fn registry_contains_all_core_ids_in_order() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("build should succeed");

    let ids = machine.registry().chip_ids();
    assert_eq!(ids, core_chip_ids::ALL.to_vec());
}

#[test]
fn registry_get_returns_correct_chip() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("build should succeed");

    let chip = machine
        .registry()
        .get(core_chip_ids::RANGE_CHECK)
        .expect("RangeCheck should be registered");
    assert_eq!(chip.chip_id(), core_chip_ids::RANGE_CHECK);
    assert!(!chip.has_interactions());

    assert!(
        machine
            .registry()
            .get(tabula_stark::chips::ChipId(999))
            .is_none()
    );
}

// ── Custom chip registration ────────────────────────────────────────────────

#[test]
fn with_chip_registers_single_chip() {
    let machine = TabulaMachine::builder()
        .with_chip(RangeCheckChip)
        .build()
        .expect("single chip should succeed");

    assert_eq!(
        machine.registry().chip_ids(),
        vec![core_chip_ids::RANGE_CHECK]
    );
}

// ── core_chips() function ────────────────────────────────────────────────────

#[test]
fn core_chips_returns_nine_chips() {
    let chips = core_chips();
    assert_eq!(chips.len(), 9);
    let ids: Vec<_> = chips.iter().map(|c| c.chip_id()).collect();
    assert_eq!(ids, core_chip_ids::ALL.to_vec());
}

// ── ChipRegistry standalone ────────────────────────────────────────────────

#[test]
fn registry_validate_empty() {
    let reg = ChipRegistry::new();
    assert!(matches!(reg.validate(), Err(SetupError::EmptyRegistry)));
}

#[test]
fn registry_validate_duplicate() {
    let mut reg = ChipRegistry::new();
    reg.register(RangeCheckChip);
    reg.register(RangeCheckChip);
    assert!(matches!(
        reg.validate(),
        Err(SetupError::DuplicateChipId(_))
    ));
}

#[test]
fn registry_validate_ok() {
    let mut reg = ChipRegistry::new();
    reg.register_core();
    assert!(reg.validate().is_ok());
}

// ── Prove/verify via TabulaMachine ───────────────────────────────────────────

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_baby_bear::BabyBear>)>>;

/// Compile DSL, execute a batch, generate witness, build traces, prove via
/// TabulaMachine, and verify.
fn machine_pipeline(
    source: &str,
    initial_cells: &[(TableId, ColId, RowKey, Value)],
    transactions: Vec<Transaction>,
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
            let (state, _com) = vc.commit_column(schema.id, col_def.id, entries).unwrap();
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

    let traces = build_trace_map::<PoseidonHasher, 3>(
        &witness,
        &program,
        &batch,
        &result,
        &schemas_by_id,
        &InMemoryStaticTables::new(),
        PoseidonHasher::new(),
    )
    .expect("trace assembly");

    let statement = PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    };

    // Build machine and prove/verify through the new API.
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("machine build should succeed");

    let proof = machine.prove(&traces, statement).expect("proving");
    assert!(
        !proof.chip_proofs.is_empty(),
        "proof should contain at least one chip proof"
    );

    machine
        .verify(&proof)
        .expect("STARK verification should succeed");
}

#[test]
fn machine_prove_verify_read_write() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    machine_pipeline(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(10)],
            sender: [7u8; 32],
            nonce: 0,
            signature: vec![],
        }],
    );
}
