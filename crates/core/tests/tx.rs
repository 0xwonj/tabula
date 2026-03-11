#![allow(missing_docs)]
use tabula_core::{Batch, Transaction, TxTypeId, Value};

#[test]
fn test_batch_borsh_round_trip() {
    let batch = Batch {
        transactions: vec![Transaction {
            tx_type: TxTypeId(1),
            params: vec![Value::U64(42)],
            sender: [1u8; 32],
            nonce: 0,
            signature: vec![0xDE, 0xAD],
        }],
    };
    let bytes = borsh::to_vec(&batch).unwrap();
    let decoded: Batch = borsh::from_slice(&bytes).unwrap();
    assert_eq!(batch, decoded);
}

#[test]
fn test_signable_bytes_excludes_signature() {
    let tx1 = Transaction {
        tx_type: TxTypeId(1),
        params: vec![Value::U64(42)],
        sender: [1u8; 32],
        nonce: 0,
        signature: vec![0xDE, 0xAD],
    };
    let tx2 = Transaction {
        tx_type: TxTypeId(1),
        params: vec![Value::U64(42)],
        sender: [1u8; 32],
        nonce: 0,
        signature: vec![0xFF, 0xFF, 0xFF],
    };
    // Different signatures must produce the same signable bytes
    assert_eq!(tx1.signable_bytes().unwrap(), tx2.signable_bytes().unwrap());
}

#[test]
fn test_signable_bytes_differs_on_nonce() {
    let tx1 = Transaction {
        tx_type: TxTypeId(1),
        params: vec![Value::U64(42)],
        sender: [1u8; 32],
        nonce: 0,
        signature: vec![],
    };
    let tx2 = Transaction {
        tx_type: TxTypeId(1),
        params: vec![Value::U64(42)],
        sender: [1u8; 32],
        nonce: 1,
        signature: vec![],
    };
    assert_ne!(tx1.signable_bytes().unwrap(), tx2.signable_bytes().unwrap());
}
