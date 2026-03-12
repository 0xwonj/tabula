use std::collections::BTreeMap;

use tabula_commitment::BabyBearCodec;
use tabula_core::traits::ValueCodec;
use tabula_core::{ColId, AccessEvent, BatchResult, OpKind, TxResult, Value};

use super::*;

// -- Init row tests --

#[test]
fn init_rows_from_read_set_present() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 10), Some(Value::U64(42)))],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![read_event(1, 0, 10, 42, 1, 0)],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(10, 42)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.columns.len(), 1);
    let col_w = &witness.columns[0];
    assert_eq!(col_w.init_rows.len(), 1);
    assert!(!col_w.init_rows[0].val_is_null);
    assert_eq!(col_w.init_rows[0].key.row, r(10));
}

#[test]
fn init_rows_from_read_set_null() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![(ck(1, 0, 5), Some(Value::U64(99)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                null_read_event(1, 0, 5, 1, 0),
                write_event(1, 0, 5, 99, 2, 0),
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let col_w = &witness.columns[0];
    assert_eq!(col_w.init_rows.len(), 1);
    assert!(col_w.init_rows[0].val_is_null);
    // Canonical zero: encoded U64(0)
    let codec = BabyBearCodec;
    let expected_fes = codec.encode(&Value::U64(0)).unwrap();
    assert_eq!(col_w.init_rows[0].value_fes, expected_fes);
}

#[test]
fn init_rows_sorted_by_key() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 30), Some(Value::U64(3))),
            (ck(1, 0, 10), Some(Value::U64(1))),
            (ck(1, 0, 20), Some(Value::U64(2))),
        ],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 30, 3, 1, 0),
                read_event(1, 0, 10, 1, 2, 0),
                read_event(1, 0, 20, 2, 3, 0),
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> =
        [column_state_with(&vc, 1, 0, &[(10, 1), (20, 2), (30, 3)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let rows = &witness.columns[0].init_rows;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].key.row.0, 10);
    assert_eq!(rows[1].key.row.0, 20);
    assert_eq!(rows[2].key.row.0, 30);
}

#[test]
fn init_rows_multi_column() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(20))),
        ],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![read_event(1, 0, 1, 10, 1, 0), read_event(1, 1, 1, 20, 2, 0)],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 1, 1, &[(1, 20)]),
    ]
    .into();
    let witness = wg.generate(&result, &schema, &states).unwrap();
    assert_eq!(witness.columns.len(), 2);
}

// -- Access row tests --

#[test]
fn access_rows_read_write() {
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

    let access = &witness.columns[0].access_rows;
    assert_eq!(access.len(), 2);
    assert!(!access[0].is_write);
    assert!(access[1].is_write);
}

#[test]
fn access_rows_null_read() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![null_read_event(1, 0, 5, 1, 0)],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let access = &witness.columns[0].access_rows;
    assert_eq!(access.len(), 1);
    assert!(access[0].val_is_null);
}

#[test]
fn access_rows_time_carried_through() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![read_event(1, 0, 1, 10, 42, 0)],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.columns[0].access_rows[0].time, 42);
}

#[test]
fn access_rows_multi_tx() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(30)))],
        txs: vec![
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![
                    read_event(1, 0, 1, 10, 1, 0),
                    write_event(1, 0, 1, 20, 2, 0),
                ],
            },
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![
                    read_event(1, 0, 1, 20, 3, 1),
                    write_event(1, 0, 1, 30, 4, 1),
                ],
            },
        ],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let access = &witness.columns[0].access_rows;
    assert_eq!(access.len(), 4);
    assert_eq!(access[0].tx_index, 0);
    assert_eq!(access[3].tx_index, 1);
}

// -- Column witness tests --

#[test]
fn column_witness_single_write() {
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

    let col_w = &witness.columns[0];
    assert!(col_w.meta.is_touched);
    assert_ne!(col_w.meta.com_old, col_w.meta.com_new);
}

#[test]
fn column_witness_delete() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), None)],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                read_event(1, 0, 1, 10, 1, 0),
                AccessEvent {
                    key: ck(1, 0, 1),
                    op: OpKind::Write,
                    value: Value::U64(0),
                    val_is_null: true,
                    time: 2,
                    effect_ordinal_in_tx: 1,
                },
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let col_w = &witness.columns[0];
    assert!(col_w.meta.is_touched);
    assert!(!col_w.meta.is_empty_old);
    assert!(col_w.meta.is_empty_new);
}

#[test]
fn column_witness_untouched() {
    let wg = make_wg();
    let vc = mock_vc();
    // Access col 0, but col 1 is untouched
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
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let states: BTreeMap<_, _> = [
        column_state_with(&vc, 1, 0, &[(1, 10)]),
        column_state_with(&vc, 1, 1, &[(1, 99)]),
    ]
    .into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    assert_eq!(witness.columns.len(), 2);
    let untouched = witness
        .columns
        .iter()
        .find(|cw| cw.col == ColId(1))
        .unwrap();
    assert!(!untouched.meta.is_touched);
    assert_eq!(untouched.meta.com_old, untouched.meta.com_new);
}

// -- ColumnMeta tests --

#[test]
fn column_meta_empty_to_nonempty() {
    let wg = make_wg();
    let vc = mock_vc();
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), None)],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(42)))],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 42, 2, 0),
            ],
        }],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
    let witness = wg.generate(&result, &schema, &states).unwrap();

    let meta = &witness.columns[0].meta;
    assert!(meta.is_empty_old);
    assert!(!meta.is_empty_new);
    assert!(meta.is_touched);
}
