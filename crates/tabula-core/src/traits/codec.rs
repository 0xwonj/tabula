//! Encoding and policy trait abstractions.

use crate::error::TabulaError;
use crate::{Value, ValueType};

/// Encodes/decodes application-level Values to/from field elements.
pub trait ValueCodec: Send + Sync {
    /// The field element representation.
    type FieldRepr: Clone + Send + Sync;

    /// Encode a Value into field elements.
    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError>;

    /// Decode field elements back into a Value.
    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        target_type: ValueType,
    ) -> Result<Value, TabulaError>;

    /// How many field elements a given ValueType requires.
    fn field_elements_per(&self, value_type: ValueType) -> usize;
}

/// Replay protection policy abstraction.
pub trait NoncePolicy: Send + Sync {
    /// Validate that a transaction's nonce is acceptable. Returns `Ok(())` on success.
    fn validate(
        &self,
        sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError>;

    /// Compute the next nonce after a successful transaction.
    fn next_nonce(&self, sender: &[u8; 32], current_nonce: u64) -> u64;
}
