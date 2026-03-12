use std::collections::BTreeMap;

use tabula_core::{BatchResult, TxResult, Value};
use tabula_witness::witness::route::KeyRoute;

use super::*;

// -- State root tests --

#[test]
fn state_root_deterministic() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![read_event(1, 0, 1, 10, 1, 0)],
        }],
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
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
        }],
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
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), None)],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(1)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 1, 2, 0),
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_ne!(witness.old_state_root, witness.new_state_root);
}

// -- End-to-end tests --

#[test]
fn e2e_full_flow_single_column() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 0, 2), Some(Value::U64(20))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(1, 0, 3), Some(Value::U64(30))),
        ],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 0, 2, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
                write_event(1, 0, 3, 30, 4, 0),
            ],
        }],
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
    assert_eq!(witness.tx_results.len(), 1);
}

#[test]
fn e2e_two_columns_multi_tx() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(100))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(1, 1, 1), Some(Value::U64(200))),
        ],
        txs: vec![
            // tx 0: read+write col 0
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![
                    read_event(1, 0, 1, 10, 1, 0),
                    write_event(1, 0, 1, 15, 2, 0),
                ],
            },
            // tx 1: read+write col 1
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![
                    read_event(1, 1, 1, 100, 3, 1),
                    write_event(1, 1, 1, 200, 4, 1),
                ],
            },
        ],
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
    assert_eq!(witness.tx_results.len(), 2);
    assert_ne!(witness.old_state_root, witness.new_state_root);
}

#[test]
fn missing_schema_returns_error() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![write_event(1, 0, 1, 10, 1, 0)],
        }],
    };
    let schema = schemas(vec![]); // no schemas!
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    assert!(wg.generate(&result, &schema, &states).is_err());
}

#[test]
fn touched_column_missing_from_old_states_returns_error() {
    let wg = make_wg();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![write_event(1, 0, 1, 10, 1, 0)],
        }],
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
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(20))),
        ],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(15)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 1, 1, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
            ],
        }],
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
fn tx_results_preserved() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        txs: vec![
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![read_event(1, 0, 1, 10, 1, 0)],
            },
            TxResult::Failed {
                reason: "overflow".into(),
                partial_events: vec![],
                failed_instruction: Some(3),
            },
        ],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_eq!(witness.tx_results.len(), 2);
    assert!(matches!(witness.tx_results[1], TxResult::Failed { .. }));
}

// -- Key routing integration --

#[test]
fn key_routes_populated() {
    let wg = make_wg();
    let vc = mock_vc();
    let k_read = ck(1, 0, 1);
    let k_write = ck(1, 0, 2);
    let result = BatchResult {
        read_set_old: vec![
            (k_read, Some(Value::U64(10))),
            (k_write, Some(Value::U64(20))),
        ],
        write_set_final: vec![(k_write, Some(Value::U64(99)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 0, 2, 20, 2, 0),
                write_event(1, 0, 2, 99, 3, 0),
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.key_routes.len(), 2);
    assert_eq!(witness.key_routes[&k_read], KeyRoute::ReadOnly);
    assert_eq!(witness.key_routes[&k_write], KeyRoute::SortedMemory);
}

// SortedMem integration tests removed -- SortedMem chip eliminated in 5-chip architecture.
// TODO(Phase 4): Add WitnessGenerator -> StateColumn integration tests.
