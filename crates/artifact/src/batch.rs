//! Batch file models and hex parsing utilities.

use serde::{Deserialize, Serialize};

use tabula_core::{Transaction, TxTypeId, Value};

use crate::ArtifactError;

/// JSON representation of a transaction batch file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchFile {
    /// Transactions in execution order.
    pub transactions: Vec<TxInput>,
}

/// One transaction input row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TxInput {
    /// Transaction type id.
    pub tx_type: u32,
    /// Typed transaction params.
    pub params: Vec<Value>,
    /// Sender as hex-encoded 32-byte key.
    pub sender: String,
    /// Replay nonce.
    pub nonce: u64,
}

impl TxInput {
    /// Convert to core transaction form.
    pub fn to_transaction(&self) -> Result<Transaction, ArtifactError> {
        let sender = parse_hex_32(&self.sender)?;
        Ok(Transaction {
            tx_type: TxTypeId(self.tx_type),
            params: self.params.clone(),
            sender,
            nonce: self.nonce,
            signature: vec![],
        })
    }
}

/// Parse a hex-encoded 32-byte value, left-padding short strings.
pub fn parse_hex_32(s: &str) -> Result<[u8; 32], ArtifactError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Ok([0u8; 32]);
    }

    let padded = format!("{:0>64}", s);
    if padded.len() != 64 {
        return Err(ArtifactError::InvalidSenderHex {
            context: "length",
            detail: format!("expected at most 64 hex chars, got {}", s.len()),
        });
    }

    let mut out = [0u8; 32];
    for (i, chunk) in padded.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| ArtifactError::InvalidSenderHex {
            context: "encoding",
            detail: e.to_string(),
        })?;
        out[i] = u8::from_str_radix(byte_str, 16).map_err(|e| ArtifactError::InvalidSenderHex {
            context: "hex digit",
            detail: e.to_string(),
        })?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_file_serde_roundtrip() {
        let batch = BatchFile {
            transactions: vec![TxInput {
                tx_type: 0,
                params: vec![Value::U64(100)],
                sender: "01".repeat(32),
                nonce: 0,
            }],
        };

        let json = serde_json::to_string(&batch).expect("serialize");
        let back: BatchFile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.transactions.len(), 1);
        assert_eq!(back.transactions[0].params[0], Value::U64(100));
    }

    #[test]
    fn tx_input_to_transaction_roundtrip() {
        let tx = TxInput {
            tx_type: 1,
            params: vec![Value::U64(42), Value::Bool(true)],
            sender: "ab".repeat(32),
            nonce: 7,
        };

        let core_tx = tx.to_transaction().expect("convert");
        assert_eq!(core_tx.tx_type, TxTypeId(1));
        assert_eq!(core_tx.params.len(), 2);
        assert_eq!(core_tx.nonce, 7);
        assert_eq!(core_tx.sender[0], 0xab);
    }

    #[test]
    fn parse_hex_32_full() {
        let hex = "01".repeat(32);
        let out = parse_hex_32(&hex).expect("hex should parse");
        assert_eq!(out, [1u8; 32]);
    }

    #[test]
    fn parse_hex_32_short_left_pad() {
        let out = parse_hex_32("ff").expect("hex should parse");
        let mut expected = [0u8; 32];
        expected[31] = 0xff;
        assert_eq!(out, expected);
    }

    #[test]
    fn parse_hex_32_with_prefix() {
        let out = parse_hex_32("0xff").expect("hex with prefix");
        let mut expected = [0u8; 32];
        expected[31] = 0xff;
        assert_eq!(out, expected);
    }

    #[test]
    fn parse_hex_32_empty() {
        let out = parse_hex_32("").expect("empty hex");
        assert_eq!(out, [0u8; 32]);
    }
}
