//! Canonical batch and transaction fixtures.

use tabula_artifact::{TransactionBatch, TransactionInput};
use tabula_core::{PortableValue, Transaction, TxTypeId};

pub fn core_tx(tx_type: u32, params: Vec<PortableValue>) -> Transaction {
    Transaction {
        tx_type: TxTypeId(tx_type),
        params,
    }
}

pub fn artifact_tx(tx_type: u32, params: Vec<PortableValue>) -> TransactionInput {
    TransactionInput { tx_type, params }
}

pub fn single_tx_batch(tx_type: u32, params: Vec<PortableValue>) -> TransactionBatch {
    TransactionBatch {
        transactions: vec![artifact_tx(tx_type, params)],
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

pub fn multi_tx_batch(
    items: impl IntoIterator<Item = (u32, Vec<PortableValue>)>,
) -> TransactionBatch {
    TransactionBatch {
        transactions: items
            .into_iter()
            .map(|(tx_type, params)| artifact_tx(tx_type, params))
            .collect(),
    }
}
