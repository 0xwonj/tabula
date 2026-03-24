#![allow(missing_docs)]
use tabula_core::{Batch, PortableValue, Transaction, TxTypeId, TypeId};

fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TypeId(0), borsh::to_vec(&value).expect("portable u64"))
}

#[test]
fn test_batch_borsh_round_trip() {
    let batch = Batch {
        transactions: vec![Transaction {
            tx_type: TxTypeId(1),
            params: vec![portable_u64(42)],
        }],
    };
    let bytes = borsh::to_vec(&batch).unwrap();
    let decoded: Batch = borsh::from_slice(&bytes).unwrap();
    assert_eq!(batch, decoded);
}

#[test]
fn test_transaction_borsh_round_trip() {
    let tx = Transaction {
        tx_type: TxTypeId(1),
        params: vec![portable_u64(42)],
    };
    let bytes = borsh::to_vec(&tx).unwrap();
    let decoded: Transaction = borsh::from_slice(&bytes).unwrap();
    assert_eq!(tx, decoded);
}
