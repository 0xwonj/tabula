//! Canonical batch and transaction fixtures.

use tabula_artifact::{TransactionBatch, TransactionInput};
use tabula_core::{Transaction, TxTypeId, Value};

pub const DEFAULT_CORE_SENDER: [u8; 32] = [1u8; 32];
pub const DEFAULT_ARTIFACT_SENDER: &str =
    "0101010101010101010101010101010101010101010101010101010101010101";

pub fn core_tx(tx_type: u32, params: Vec<Value>, nonce: u64) -> Transaction {
    core_tx_with_sender(tx_type, params, DEFAULT_CORE_SENDER, nonce)
}

pub fn core_tx_with_sender(
    tx_type: u32,
    params: Vec<Value>,
    sender: [u8; 32],
    nonce: u64,
) -> Transaction {
    Transaction {
        tx_type: TxTypeId(tx_type),
        params,
        sender,
        nonce,
        signature: vec![],
    }
}

pub fn artifact_tx(tx_type: u32, params: Vec<Value>, nonce: u64) -> TransactionInput {
    artifact_tx_with_sender(tx_type, params, DEFAULT_ARTIFACT_SENDER.to_string(), nonce)
}

pub fn artifact_tx_with_sender(
    tx_type: u32,
    params: Vec<Value>,
    sender: String,
    nonce: u64,
) -> TransactionInput {
    TransactionInput {
        tx_type,
        params,
        sender,
        nonce,
    }
}

pub fn single_tx_batch(tx_type: u32, params: Vec<Value>) -> TransactionBatch {
    TransactionBatch {
        transactions: vec![artifact_tx(tx_type, params, 0)],
    }
}

pub fn no_param_batch(tx_type: u32) -> TransactionBatch {
    single_tx_batch(tx_type, vec![])
}

pub fn empty_batch() -> TransactionBatch {
    TransactionBatch {
        transactions: vec![],
    }
}

pub fn multi_tx_batch(items: impl IntoIterator<Item = (u32, Vec<Value>, u64)>) -> TransactionBatch {
    TransactionBatch {
        transactions: items
            .into_iter()
            .map(|(tx_type, params, nonce)| artifact_tx(tx_type, params, nonce))
            .collect(),
    }
}
