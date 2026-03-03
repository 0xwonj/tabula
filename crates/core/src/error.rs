//! Error types for the Tabula kernel.

use crate::{CellKey, ColId, RowKey, TableId};

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
    TxTypeNotFound(crate::TxTypeId),

    /// Borsh serialization/deserialization error.
    #[error("borsh encoding error: {0}")]
    BorshEncodingError(String),

    /// Field-element codec error (limb ranges, BabyBear canonical checks).
    #[error("field encoding error: {0}")]
    FieldEncodingError(String),

    /// Row key not found in table.
    #[error("row not found: {0:?} {1:?}")]
    RowNotFound(TableId, RowKey),

    /// Consistency check failure.
    #[error("consistency error: {0}")]
    ConsistencyError(String),

    /// Proof-layer trace construction or validation failure.
    #[error("proof error: [{phase}] {detail}")]
    ProofError {
        /// Which proof phase failed.
        phase: &'static str,
        /// Human-readable detail.
        detail: String,
    },

    /// IR body validation / type inference failure.
    #[error("invalid IR: {0}")]
    InvalidIr(String),

    /// Transaction parameter count or type does not match schema.
    #[error("param schema mismatch: {0}")]
    ParamSchemaMismatch(String),

    /// NF-1: duplicate Read to the same (table, col, row) in one tx body.
    #[error(
        "NF-1 unique-read: instructions {first} and {second} both read (table {table:?}, col {col:?})"
    )]
    NfUniqueRead {
        /// First Read instruction index.
        first: usize,
        /// Second Read instruction index.
        second: usize,
        /// Table of the duplicated access.
        table: TableId,
        /// Column of the duplicated access.
        col: ColId,
    },

    /// NF-2: duplicate Write to the same (table, col, row) in one tx body.
    #[error(
        "NF-2 unique-write: instructions {first} and {second} both write (table {table:?}, col {col:?})"
    )]
    NfUniqueWrite {
        /// First Write instruction index.
        first: usize,
        /// Second Write instruction index.
        second: usize,
        /// Table of the duplicated access.
        table: TableId,
        /// Column of the duplicated access.
        col: ColId,
    },

    /// NF-3: Read after a prior Write to the same (table, col, row).
    #[error(
        "NF-3 read-after-write: read at {read_at} after write at {write_at} to (table {table:?}, col {col:?})"
    )]
    NfReadAfterWrite {
        /// Write instruction index.
        write_at: usize,
        /// Read instruction index (must be > write_at).
        read_at: usize,
        /// Table of the access.
        table: TableId,
        /// Column of the access.
        col: ColId,
    },

    /// NF-4: two accesses to the same (table, col) have unresolvable row expressions.
    #[error(
        "NF-4 ambiguous-alias: instructions {first} and {second} access (table {table:?}, col {col:?}) with unresolvable row expressions"
    )]
    NfAmbiguousAlias {
        /// First access instruction index.
        first: usize,
        /// Second access instruction index.
        second: usize,
        /// Table of the access.
        table: TableId,
        /// Column of the access.
        col: ColId,
    },

    /// Custom error for extension points.
    #[error("{0}")]
    Custom(String),
}
