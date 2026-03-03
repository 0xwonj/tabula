use std::collections::BTreeMap;

use tabula_commitment::{ColumnState, HybridVC, PoseidonHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{
    ColId, ExecutionEvent, ExecutionResult, OpKind, RowKey, TableId, TableSchema, TxOutcome, Value,
};

use tabula_commitment::BabyBearCodec;
use tabula_proof::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::chips::column_meta::trace::generate_column_meta_trace;
use tabula_proof::debug::debug_check;
use tabula_proof::witness::WitnessGenerator;
use tabula_proof::witness::route::KeyRoute;

use super::*;

// ── State root tests ──

#[test]
fn state_root_deterministic() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        events: vec![read_event(1, 0, 1, 10, 1, 0)],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let w1 = wg.generate(&result, &schema, &states).unwrap();
    let w2 = wg.generate(&result, &schema, &states).unwrap();
    assert_eq!(w1.old_state_root, w2.old_state_root);
    assert_eq!(w1.new_state_root, w2.new_state_root);
}

#[test]
fn state_root_changes_on_write() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            write_event(1, 0, 1, 20, 2, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_ne!(witness.old_state_root, witness.new_state_root);
}

#[test]
fn state_root_empty_state() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), None)],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(1)))],
        events: vec![
            null_read_event(1, 0, 1, 1, 0),
            write_event(1, 0, 1, 1, 2, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_ne!(witness.old_state_root, witness.new_state_root);
}

// ── End-to-end tests ──

#[test]
fn e2e_full_flow_single_column() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 0, 2), Some(Value::U64(20))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(1, 0, 3), Some(Value::U64(30))),
        ],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            read_event(1, 0, 2, 20, 2, 0),
            write_event(1, 0, 1, 15, 3, 0),
            write_event(1, 0, 3, 30, 4, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.columns.len(), 1);
    let col_w = &witness.columns[0];
    assert_eq!(col_w.init_rows.len(), 2);
    assert_eq!(col_w.access_rows.len(), 4);
    assert!(col_w.meta.is_touched);
    assert!(col_w.merge_trace.is_some());
    assert_eq!(witness.tx_outcomes.len(), 1);
}

#[test]
fn e2e_two_columns_multi_tx() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(100))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(1, 1, 1), Some(Value::U64(200))),
        ],
        events: vec![
            // tx 0: read+write col 0
            read_event(1, 0, 1, 10, 1, 0),
            write_event(1, 0, 1, 15, 2, 0),
            // tx 1: read+write col 1
            read_event(1, 1, 1, 100, 3, 1),
            write_event(1, 1, 1, 200, 4, 1),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 1, 1, &[(1, 100)]),
    ]
    .into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.columns.len(), 2);
    assert!(witness.columns.iter().all(|c| c.meta.is_touched));
    assert_eq!(witness.tx_outcomes.len(), 2);
    assert_ne!(witness.old_state_root, witness.new_state_root);
}

#[test]
fn missing_schema_returns_error() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        events: vec![write_event(1, 0, 1, 10, 1, 0)],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![]); // no schemas!
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    assert!(wg.generate(&result, &schema, &states).is_err());
}

#[test]
fn touched_column_missing_from_old_states_returns_error() {
    let wg = make_wg();
    let result = ExecutionResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        events: vec![write_event(1, 0, 1, 10, 1, 0)],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = BTreeMap::new(); // no column states!
    let err = wg.generate(&result, &schema, &states).unwrap_err();
    assert!(err.to_string().contains("not in old_column_states"));
}

#[test]
fn column_metas_populated_and_sorted() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(20))),
        ],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(15)))],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            read_event(1, 1, 1, 20, 2, 0),
            write_event(1, 0, 1, 15, 3, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 1, 1, &[(1, 20)]),
    ]
    .into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    // column_metas should have 2 entries, sorted by (table, col).
    assert_eq!(witness.column_metas.len(), 2);
    assert_eq!(witness.column_metas[0].table, t(1));
    assert_eq!(witness.column_metas[0].col, c(0));
    assert_eq!(witness.column_metas[1].table, t(1));
    assert_eq!(witness.column_metas[1].col, c(1));
    // col 0 was written to, col 1 was only read.
    assert!(witness.column_metas[0].is_touched);
    assert!(witness.column_metas[1].is_touched);
}

#[test]
fn tx_outcomes_preserved() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        events: vec![read_event(1, 0, 1, 10, 1, 0)],
        emitted: vec![],
        tx_outcomes: vec![
            TxOutcome::Success,
            TxOutcome::Failed {
                reason: "overflow".into(),
                partial_events: vec![],
                failed_instruction: Some(3),
            },
        ],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_eq!(witness.tx_outcomes.len(), 2);
    assert!(matches!(witness.tx_outcomes[1], TxOutcome::Failed { .. }));
}

// ── M5↔M6 integration: WitnessGenerator → ColumnMeta AIR ──

/// Helper: generate witness, build ColumnMeta trace, verify AIR constraints.
fn assert_column_meta_air_valid(
    result: &ExecutionResult,
    schema: &BTreeMap<TableId, TableSchema>,
    states: &BTreeMap<(TableId, ColId), ColumnState<tabula_commitment::MockFieldHasher>>,
) {
    let wg = make_wg();
    let witness = wg.generate(result, schema, states).unwrap();
    let trace = generate_column_meta_trace(&witness.column_metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect("ColumnMeta AIR constraints should pass for witness-generated trace");
}

// ── PoseidonHasher variants for tests involving empty columns ──
//
// The ColumnMeta AIR's Com_empty constraint verifies `com = Poseidon(0x00||t||c)`.
// MockFieldHasher produces different hashes, so tests with empty columns must
// use PoseidonHasher to produce protocol-compatible commitments.

fn poseidon_vc() -> HybridVC<PoseidonHasher> {
    HybridVC::new(PoseidonHasher::new(), 100)
}

fn empty_column_state_poseidon(
    vc: &HybridVC<PoseidonHasher>,
    table: u32,
    col: u16,
) -> ((TableId, ColId), ColumnState<PoseidonHasher>) {
    let (state, _) = vc.commit_column(t(table), c(col), vec![]).unwrap();
    ((t(table), c(col)), state)
}

fn column_state_with_poseidon(
    vc: &HybridVC<PoseidonHasher>,
    table: u32,
    col: u16,
    entries: &[(u64, u64)],
) -> ((TableId, ColId), ColumnState<PoseidonHasher>) {
    let codec = BabyBearCodec;
    let enc: Vec<(RowKey, Vec<p3_baby_bear::BabyBear>)> = entries
        .iter()
        .map(|&(k, v)| (r(k), codec.encode(&Value::U64(v)).unwrap()))
        .collect();
    let (state, _) = vc.commit_column(t(table), c(col), enc).unwrap();
    ((t(table), c(col)), state)
}

fn assert_column_meta_air_valid_poseidon(
    result: &ExecutionResult,
    schema: &BTreeMap<TableId, TableSchema>,
    states: &BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>,
) {
    let wg = WitnessGenerator::new(poseidon_vc());
    let witness = wg.generate(result, schema, states).unwrap();
    let trace = generate_column_meta_trace(&witness.column_metas, &Default::default());
    debug_check(&ColumnMetaChip, &trace)
        .expect("ColumnMeta AIR constraints should pass for witness-generated trace");
}

#[test]
fn m5_m6_single_column_write() {
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            write_event(1, 0, 1, 20, 2, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    assert_column_meta_air_valid(&result, &schema, &states);
}

#[test]
fn m5_m6_multi_column_touched_and_untouched() {
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            write_event(1, 0, 1, 20, 2, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 1, 1, &[(1, 99)]),
    ]
    .into();
    assert_column_meta_air_valid(&result, &schema, &states);
}

#[test]
fn m5_m6_empty_to_nonempty() {
    let vc = poseidon_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), None)],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(42)))],
        events: vec![
            null_read_event(1, 0, 1, 1, 0),
            write_event(1, 0, 1, 42, 2, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state_poseidon(&vc, 1, 0)].into();
    assert_column_meta_air_valid_poseidon(&result, &schema, &states);
}

#[test]
fn m5_m6_delete() {
    let vc = poseidon_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), None)],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            ExecutionEvent {
                key: ck(1, 0, 1),
                op: OpKind::Write,
                value: Value::U64(0),
                val_is_null: true,
                time: 2,
                tx_index: 0,
                effect_ordinal_in_tx: 1,
            },
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with_poseidon(&vc, 1, 0, &[(1, 10)])].into();
    assert_column_meta_air_valid_poseidon(&result, &schema, &states);
}

#[test]
fn m5_m6_multi_table_multi_tx() {
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(2, 0, 1), Some(Value::U64(100))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(2, 0, 1), Some(Value::U64(200))),
        ],
        events: vec![
            // tx 0: read+write table 1
            read_event(1, 0, 1, 10, 1, 0),
            write_event(1, 0, 1, 15, 2, 0),
            // tx 1: read+write table 2
            read_event(2, 0, 1, 100, 3, 1),
            write_event(2, 0, 1, 200, 4, 1),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0]), u64_schema(2, &[0])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 2, 0, &[(1, 100)]),
    ]
    .into();
    assert_column_meta_air_valid(&result, &schema, &states);
}

#[test]
fn m5_m6_read_only_no_writes() {
    let vc = mock_vc();
    let result = ExecutionResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        events: vec![read_event(1, 0, 1, 10, 1, 0)],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    assert_column_meta_air_valid(&result, &schema, &states);
}

// ── Key routing integration ──

#[test]
fn key_routes_populated() {
    let wg = make_wg();
    let vc = mock_vc();
    let k_read = ck(1, 0, 1);
    let k_write = ck(1, 0, 2);
    let result = ExecutionResult {
        read_set_old: vec![
            (k_read, Some(Value::U64(10))),
            (k_write, Some(Value::U64(20))),
        ],
        write_set_final: vec![(k_write, Some(Value::U64(99)))],
        events: vec![
            read_event(1, 0, 1, 10, 1, 0),
            read_event(1, 0, 2, 20, 2, 0),
            write_event(1, 0, 2, 99, 3, 0),
        ],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.key_routes.len(), 2);
    assert_eq!(witness.key_routes[&k_read], KeyRoute::ReadOnly);
    assert_eq!(witness.key_routes[&k_write], KeyRoute::SortedMemory);
}

// SortedMem integration tests removed — SortedMem chip eliminated in 5-chip architecture.
// TODO(Phase 4): Add WitnessGenerator → StateColumn integration tests.
