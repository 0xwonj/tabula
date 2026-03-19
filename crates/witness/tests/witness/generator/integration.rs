use std::collections::BTreeMap;

use tabula_commitment::{ColumnMeta, ColumnState, MockFieldHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{BatchResult, ColId, RowKey, TableId, TxResult, Value};
use tabula_witness::proof_column_commitment;

use super::*;

fn build_meta(
    table: TableId,
    col: ColId,
    old_state: &ColumnState<MockFieldHasher>,
    writes: &[(RowKey, Option<Vec<p3_koala_bear::KoalaBear>>)],
    is_touched: bool,
) -> ColumnMeta {
    let com_old = proof_column_commitment(table, col, old_state).expect("old commitment");
    let tag = old_state.scheme_tag();
    let is_empty_old = old_state.is_empty();
    let (new_state, _, _) = if is_touched {
        old_state.apply_writes(&MockFieldHasher, table, col, writes)
    } else {
        (old_state.clone(), com_old, None)
    };
    let com_new = proof_column_commitment(table, col, &new_state).expect("new commitment");

    ColumnMeta {
        table,
        col,
        tag,
        com_old,
        com_new,
        is_empty_old,
        is_empty_new: new_state.is_empty(),
        is_touched,
    }
}

#[test]
fn state_root_deterministic_for_same_metadata() {
    let preparer = make_preparer();
    let meta = build_meta(
        t(1),
        c(0),
        &column_state_with(1, 0, &[(1, 10)]).1,
        &[],
        false,
    );

    let roots_1 = preparer.compute_state_roots_from_metas(std::slice::from_ref(&meta));
    let roots_2 = preparer.compute_state_roots_from_metas(std::slice::from_ref(&meta));

    assert_eq!(roots_1, roots_2);
}

#[test]
fn state_root_changes_when_column_commitment_changes() {
    let preparer = make_preparer();
    let old_state = column_state_with(1, 0, &[(1, 10)]).1;
    let unchanged = build_meta(t(1), c(0), &old_state, &[], false);
    let changed = build_meta(
        t(1),
        c(0),
        &old_state,
        &[(
            r(1),
            Some(
                tabula_commitment::KoalaBearCodec
                    .encode(&Value::U64(20))
                    .unwrap(),
            ),
        )],
        true,
    );

    let roots_old = preparer.compute_state_roots_from_metas(std::slice::from_ref(&unchanged));
    let roots_new = preparer.compute_state_roots_from_metas(std::slice::from_ref(&changed));

    assert_ne!(roots_old.1, roots_new.1);
}

#[test]
fn state_root_handles_empty_to_non_empty_transition() {
    let preparer = make_preparer();
    let meta = build_meta(
        t(1),
        c(0),
        &empty_column_state(1, 0).1,
        &[(
            r(1),
            Some(
                tabula_commitment::KoalaBearCodec
                    .encode(&Value::U64(1))
                    .unwrap(),
            ),
        )],
        true,
    );

    let (old_root, new_root) = preparer.compute_state_roots_from_metas(std::slice::from_ref(&meta));

    assert_ne!(old_root, new_root);
}

#[test]
fn prepared_inputs_and_roots_cover_multiple_columns() {
    let preparer = make_preparer();
    let result = BatchResult {
        read_set_old: vec![
            (ck(1, 0, 1), Some(Value::U64(10))),
            (ck(1, 1, 1), Some(Value::U64(20))),
        ],
        write_set_final: vec![
            (ck(1, 0, 1), Some(Value::U64(15))),
            (ck(1, 1, 2), Some(Value::U64(25))),
        ],
        txs: vec![TxResult::success(
            vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 1, 1, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
                write_event(1, 1, 2, 25, 4, 0),
            ],
            vec![],
        )],
    };
    let schema = schemas(vec![u64_schema(1, &[0, 1])]);
    let prepared = preparer
        .prepare_execution_inputs(&result, &schema, [(t(1), c(0)), (t(1), c(1))].iter())
        .expect("prepared execution inputs");

    assert_eq!(prepared.init_rows_by_col.len(), 2);
    assert_eq!(prepared.access_rows_by_col.len(), 2);
    assert_eq!(prepared.writes_by_col.len(), 2);

    let states: BTreeMap<_, _> = [
        column_state_with(1, 0, &[(1, 10)]),
        column_state_with(1, 1, &[(1, 20)]),
    ]
    .into();
    let metas: Vec<_> = states
        .into_iter()
        .map(|((table, col), state)| {
            let writes = prepared
                .writes_by_col
                .get(&(table, col))
                .cloned()
                .unwrap_or_default();
            build_meta(
                table,
                col,
                &state,
                &writes,
                prepared.touched.contains(&(table, col)),
            )
        })
        .collect();
    let (old_root, new_root) = preparer.compute_state_roots_from_metas(&metas);

    assert_ne!(old_root, new_root);
    assert_eq!(metas.len(), 2);
}
