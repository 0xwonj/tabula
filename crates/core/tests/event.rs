#![allow(missing_docs)]
use tabula_core::{
    CellKey, ColId, ExecutionEvent, ExecutionResult, OpKind, RowKey, TableId, TxOutcome, Value,
};

#[test]
fn test_execution_event_borsh_round_trip() {
    let event = ExecutionEvent {
        key: CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        },
        op: OpKind::Read,
        value: Value::U64(100),
        val_is_null: false,
        time: 1,
        tx_index: 0,
        effect_ordinal_in_tx: 0,
    };
    let bytes = borsh::to_vec(&event).unwrap();
    let decoded: ExecutionEvent = borsh::from_slice(&bytes).unwrap();
    assert_eq!(event, decoded);
}

#[test]
fn test_execution_result_construction() {
    let result = ExecutionResult {
        read_set_old: vec![],
        write_set_final: vec![],
        events: vec![],
        emitted: vec![],
        tx_outcomes: vec![TxOutcome::Success],
    };
    assert_eq!(result.tx_outcomes.len(), 1);
}
