use tabula_commitment::KoalaBearCodec;
use tabula_core::traits::ValueCodec;
use tabula_core::{AccessEvent, BatchResult, ColId, OpKind, TableId, TxResult, Value};

use super::*;

fn prepare(
    result: &BatchResult,
    schema: &std::collections::BTreeMap<TableId, tabula_core::TableSchema>,
    planned: &[(TableId, ColId)],
) -> tabula_witness::PreparedExecutionInputs {
    make_preparer()
        .prepare_execution_inputs(result, schema, planned.iter())
        .expect("prepared execution inputs")
}

#[test]
fn init_rows_from_read_set_present() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 10), Some(Value::U64(42)))],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![read_event(1, 0, 10, 42, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared.init_rows_by_col[&(t(1), c(0))];
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].val_is_null);
    assert_eq!(rows[0].key.row, r(10));
}

#[test]
fn init_rows_from_read_set_null_are_canonical_zero() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![(ck(1, 0, 5), Some(Value::U64(99)))],
        txs: vec![TxResult::success(
            vec![
                null_read_event(1, 0, 5, 1, 0),
                write_event(1, 0, 5, 99, 2, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared.init_rows_by_col[&(t(1), c(0))];
    assert_eq!(rows.len(), 1);
    assert!(rows[0].val_is_null);
    assert_eq!(
        rows[0].value_fes,
        KoalaBearCodec.encode(&Value::U64(0)).unwrap()
    );
}

#[test]
fn init_rows_are_sorted_by_key() {
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 30), Some(Value::U64(3))),
            (ck(1, 0, 10), Some(Value::U64(1))),
            (ck(1, 0, 20), Some(Value::U64(2))),
        ],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 30, 3, 1, 0),
                read_event(1, 0, 10, 1, 2, 0),
                read_event(1, 0, 20, 2, 3, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let rows = &prepared.init_rows_by_col[&(t(1), c(0))];
    assert_eq!(
        rows.iter().map(|row| row.key.row.0).collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
}

#[test]
fn access_rows_preserve_event_order_and_metadata() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(30)))],
        txs: vec![
            TxResult::success(
                vec![
                    read_event(1, 0, 1, 10, 1, 0),
                    write_event(1, 0, 1, 20, 2, 0),
                ],
                vec![],
            ),
            TxResult::success(
                vec![
                    read_event(1, 0, 1, 20, 3, 1),
                    write_event(1, 0, 1, 30, 4, 1),
                ],
                vec![],
            ),
        ],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let access = &prepared.access_rows_by_col[&(t(1), c(0))];
    assert_eq!(access.len(), 4);
    assert!(!access[0].is_write);
    assert!(access[1].is_write);
    assert_eq!(access[0].tx_index, 0);
    assert_eq!(access[3].tx_index, 1);
    assert_eq!(access[0].time, 1);
    assert_eq!(access[3].time, 4);
}

#[test]
fn null_reads_remain_null_in_access_rows() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 5), None)],
        write_set_final: vec![],
        txs: vec![TxResult::success(
            vec![null_read_event(1, 0, 5, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let access = &prepared.access_rows_by_col[&(t(1), c(0))];
    assert_eq!(access.len(), 1);
    assert!(access[0].val_is_null);
}

#[test]
fn writes_are_grouped_and_sorted() {
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![
            (ck(1, 0, 30), Some(Value::U64(3))),
            (ck(1, 0, 10), Some(Value::U64(1))),
            (ck(1, 0, 20), None),
        ],
        txs: vec![TxResult::success(
            vec![
                write_event(1, 0, 30, 3, 1, 0),
                write_event(1, 0, 10, 1, 2, 0),
                AccessEvent {
                    key: ck(1, 0, 20),
                    op: OpKind::Write,
                    value: Value::U64(0),
                    val_is_null: true,
                    time: 3,
                    effect_ordinal_in_tx: 2,
                },
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let prepared = prepare(&result, &schema, &[(t(1), c(0))]);

    let writes = &prepared.writes_by_col[&(t(1), c(0))];
    assert_eq!(
        writes.iter().map(|(row, _)| row.0).collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
    assert!(writes[1].1.is_none());
}

#[test]
fn touched_columns_include_reads_and_writes_only() {
    let result = BatchResult {
        read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        write_set_final: vec![(ck(1, 1, 2), Some(Value::U64(20)))],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 1, 2, 20, 2, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1, 2])]);
    let prepared = prepare(
        &result,
        &schema,
        &[(t(1), c(0)), (t(1), c(1)), (t(1), c(2))],
    );

    assert!(prepared.touched.contains(&(t(1), c(0))));
    assert!(prepared.touched.contains(&(t(1), c(1))));
    assert!(!prepared.touched.contains(&(t(1), c(2))));
    assert!(prepared.type_map.contains_key(&(t(1), c(2))));
}

#[test]
fn missing_schema_returns_error() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        txs: vec![TxResult::success(
            vec![write_event(1, 0, 1, 10, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![]);

    assert!(
        preparer
            .prepare_execution_inputs(&result, &schema, [(t(1), c(0))].iter())
            .is_err()
    );
}

#[test]
fn touched_column_missing_from_planned_columns_returns_error() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
        txs: vec![TxResult::success(
            vec![write_event(1, 0, 1, 10, 1, 0)],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0])]);
    let err = preparer
        .prepare_execution_inputs(&result, &schema, std::iter::empty())
        .err()
        .expect("missing planned column error");

    assert!(err.to_string().contains("not in planned columns"));
}
