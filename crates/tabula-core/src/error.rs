//! Error types for the Tabula kernel.

use crate::types::{CellKey, ColId, RowKey, TableId};

/// Unified error type for all Tabula crate boundaries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TabulaError {
    /// Referenced table does not exist.
    #[error("table not found: {0:?}")]
    TableNotFound(TableId),

    /// Referenced column does not exist.
    #[error("column not found: {0:?} {1:?}")]
    ColumnNotFound(TableId, ColId),

    /// Referenced cell does not exist.
    #[error("cell not found: {0:?}")]
    CellNotFound(CellKey),

    /// Type mismatch during arithmetic or comparison.
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// The expected type description.
        expected: &'static str,
        /// The actual type description.
        actual: &'static str,
    },

    /// Arithmetic overflow (e.g. u64 addition).
    #[error("arithmetic overflow")]
    ArithmeticOverflow,

    /// Division by zero.
    #[error("division by zero")]
    DivisionByZero,

    /// An ASSERT instruction evaluated to false.
    #[error("assertion failed: {0}")]
    AssertionFailed(String),

    /// Slot index out of bounds.
    #[error("slot out of bounds: {index} (max {max})")]
    SlotOutOfBounds {
        /// The requested slot index.
        index: u16,
        /// The maximum valid slot index.
        max: u16,
    },

    /// Parameter index out of bounds.
    #[error("param out of bounds: {index} (max {max})")]
    ParamOutOfBounds {
        /// The requested parameter index.
        index: u16,
        /// The maximum valid parameter index.
        max: u16,
    },

    /// Invalid nonce for the sender.
    #[error("invalid nonce: expected {expected}, got {actual}")]
    InvalidNonce {
        /// The sender's public key.
        sender: [u8; 32],
        /// The expected nonce value.
        expected: u64,
        /// The actual nonce value.
        actual: u64,
    },

    /// Signature verification failed.
    #[error("signature invalid")]
    SignatureInvalid,

    /// Transaction type not found in program.
    #[error("tx type not found: {0:?}")]
    TxTypeNotFound(crate::tx::TxTypeId),

    /// Operation on a null value where non-null was required.
    #[error("null value in operation")]
    NullValue,

    /// Encoding/decoding error.
    #[error("encoding error: {0}")]
    EncodingError(String),

    /// Row key not found in table.
    #[error("row not found: {0:?} {1:?}")]
    RowNotFound(TableId, RowKey),

    /// Consistency check failure.
    #[error("consistency error: {0}")]
    ConsistencyError(String),

    /// IR body validation / type inference failure.
    #[error("invalid IR: {0}")]
    InvalidIr(String),

    /// Transaction parameter count or type does not match schema.
    #[error("param schema mismatch: {0}")]
    ParamSchemaMismatch(String),

    /// Custom error for extension points.
    #[error("{0}")]
    Custom(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_table_not_found() {
        let e = TabulaError::TableNotFound(TableId(7));
        assert!(e.to_string().contains("TableId(7)"));
    }

    #[test]
    fn test_error_display_arithmetic_overflow() {
        let e = TabulaError::ArithmeticOverflow;
        assert_eq!(e.to_string(), "arithmetic overflow");
    }

    #[test]
    fn test_error_display_slot_out_of_bounds() {
        let e = TabulaError::SlotOutOfBounds { index: 10, max: 5 };
        assert_eq!(e.to_string(), "slot out of bounds: 10 (max 5)");
    }

    #[test]
    fn test_error_display_custom() {
        let e = TabulaError::Custom("something went wrong".into());
        assert_eq!(e.to_string(), "something went wrong");
    }
}
