#![allow(missing_docs)]
use tabula_core::{
    AccessEvent, BatchReport, ColId, CommittedCellKey, CommittedKey, OpKind, PortableValue,
    TableId, TxResult, TypeId,
};

fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TypeId(0), borsh::to_vec(&value).expect("portable u64"))
}

#[test]
fn test_execution_event_borsh_round_trip() {
    let event = AccessEvent {
        key: CommittedCellKey {
            table: TableId(1),
            col: ColId(0),
            key: CommittedKey(vec![0; 8]),
        },
        op: OpKind::Read,
        value: portable_u64(100),
        val_is_null: false,
        time: 1,
        effect_ordinal_in_tx: 0,
    };
    let bytes = borsh::to_vec(&event).unwrap();
    let decoded: AccessEvent = borsh::from_slice(&bytes).unwrap();
    assert_eq!(event, decoded);
}

#[test]
fn test_batch_result_construction() {
    let result = BatchReport {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![TxResult::success(vec![], vec![])],
    };
    assert_eq!(result.txs.len(), 1);
    assert!(result.txs[0].is_success());
}

#[test]
fn test_batch_result_successful_events() {
    let event = AccessEvent {
        key: CommittedCellKey {
            table: TableId(1),
            col: ColId(0),
            key: CommittedKey(vec![0; 8]),
        },
        op: OpKind::Read,
        value: portable_u64(42),
        val_is_null: false,
        time: 1,
        effect_ordinal_in_tx: 0,
    };
    let result = BatchReport {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![
            TxResult::success(vec![event.clone()], vec![]),
            TxResult::Failed {
                reason: "test".into(),
                partial_events: vec![],
                failed_instruction: None,
            },
        ],
    };
    let events: Vec<_> = result.successful_events().collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, portable_u64(42));
}
