//! Transaction batch models.

use serde::{Deserialize, Serialize};

use tabula_core::{PortableValue, Transaction, TxTypeId};
use tabula_types::TypeRuntimeRegistry;

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

/// Canonical transaction batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionBatch {
    /// Transactions in execution order.
    pub transactions: Vec<TransactionInput>,
}

/// One transaction input row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionInput {
    /// Transaction type id.
    pub tx_type: u32,
    /// Typed transaction params.
    pub params: Vec<PortableValue>,
}

impl TransactionBatch {
    /// Serialize this batch into canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        canonical_json_bytes(self)
    }

    /// Compute the canonical digest bytes for this batch.
    pub fn canonical_digest_bytes(&self) -> Result<[u8; 32], ArtifactError> {
        canonical_json_digest("batch", self)
    }

    /// Compute the canonical digest hex string for this batch.
    pub fn canonical_digest(&self) -> Result<String, ArtifactError> {
        Ok(bytes_to_hex(&self.canonical_digest_bytes()?))
    }
}

impl TransactionInput {
    /// Convert to core transaction form.
    pub fn to_transaction(
        &self,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Transaction, ArtifactError> {
        for param in &self.params {
            type_runtimes.decode_portable(param).map_err(|err| {
                ArtifactError::InvalidPortableValue {
                    detail: err.to_string(),
                }
            })?;
        }
        Ok(Transaction {
            tx_type: TxTypeId(self.tx_type),
            params: self.params.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_types::{TypeRuntimeRegistry, bool_portable, u64_portable};

    #[test]
    fn batch_file_serde_roundtrip() {
        let batch = TransactionBatch {
            transactions: vec![TransactionInput {
                tx_type: 0,
                params: vec![u64_portable(100)],
            }],
        };

        let json = serde_json::to_string(&batch).expect("serialize");
        let back: TransactionBatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.transactions.len(), 1);
        assert_eq!(back.transactions[0].params[0], u64_portable(100));
    }

    #[test]
    fn tx_input_to_transaction_roundtrip() {
        let tx = TransactionInput {
            tx_type: 1,
            params: vec![u64_portable(42), bool_portable(true)],
        };

        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let core_tx = tx.to_transaction(&runtimes).expect("convert");
        assert_eq!(core_tx.tx_type, TxTypeId(1));
        assert_eq!(core_tx.params.len(), 2);
    }

    #[test]
    fn canonical_digest_is_deterministic() {
        let batch = TransactionBatch {
            transactions: vec![TransactionInput {
                tx_type: 0,
                params: vec![u64_portable(1)],
            }],
        };

        assert_eq!(
            batch.canonical_digest().expect("first"),
            batch.canonical_digest().expect("second")
        );
    }
}
