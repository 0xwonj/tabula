//! Runtime transaction types: concrete transactions and batches.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{PortableValue, TxTypeId};

/// A concrete transaction in a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Transaction {
    /// Which transaction type to execute.
    pub tx_type: TxTypeId,
    /// Concrete parameter values.
    pub params: Vec<PortableValue>,
    /// Sender's public key.
    pub sender: [u8; 32],
    /// Replay-protection nonce.
    pub nonce: u64,
    /// Cryptographic signature over the transaction.
    pub signature: Vec<u8>,
}

impl Transaction {
    /// Serialize the signable portion of the transaction (excludes signature).
    ///
    /// This is the message that should be signed and verified.
    pub fn signable_bytes(&self) -> Result<Vec<u8>, crate::error::TabulaError> {
        // Serialize (tx_type, params, sender, nonce) — NOT signature
        let signable = (self.tx_type, &self.params, self.sender, self.nonce);
        borsh::to_vec(&signable)
            .map_err(|e| crate::error::TabulaError::BorshEncodingError(e.to_string()))
    }
}

/// Program-level resource budgets for DoS prevention.
///
/// Verified by prover and verifier to ensure programs do not exceed
/// allocated resources.
///
/// **Status: data structure only** — enforcement is not yet implemented
/// in the executor or IR validation pipeline. See semantics-spec §1.8.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct ProgramBudgets {
    /// Maximum number of IR instructions per transaction.
    pub max_ops: u32,
    /// Maximum number of SSA slots per transaction.
    pub max_slots: u16,
    /// Maximum number of state accesses (reads + writes) per transaction.
    pub max_accesses: u32,
}

/// An ordered batch of transactions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Batch {
    /// The transactions in execution order.
    pub transactions: Vec<Transaction>,
}
