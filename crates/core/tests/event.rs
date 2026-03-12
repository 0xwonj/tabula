#![allow(missing_docs)]
use tabula_core::{
    AccessEvent, BatchResult, CellKey, ColId, OpKind, RowKey, TableId, TxResult, Value,
};

#[test]
fn test_execution_event_borsh_round_trip() {
    let event = AccessEvent {
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
    let decoded: AccessEvent = borsh::from_slice(&bytes).unwrap();
    assert_eq!(event, decoded);
}

#[test]
fn test_batch_result_construction() {
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![],
        }],
    };
    assert_eq!(result.txs.len(), 1);
    assert!(result.txs[0].is_success());
}

#[test]
fn test_batch_result_successful_events() {
    let event = AccessEvent {
        key: CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        },
        op: OpKind::Read,
        value: Value::U64(42),
        val_is_null: false,
        time: 1,
        tx_index: 0,
        effect_ordinal_in_tx: 0,
    };
    let result = BatchResult {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![
            TxResult::Success {
                emitted: vec![],
                access_trace: vec![event.clone()],
            },
            TxResult::Failed {
                reason: "test".into(),
                partial_events: vec![],
                failed_instruction: None,
            },
        ],
    };
    let events: Vec<_> = result.successful_events().collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::U64(42));
}
