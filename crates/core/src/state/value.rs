//! Application-level value types and arithmetic operations.

use std::cmp::Ordering;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::error::TabulaError;

/// A typed value stored in a table cell.
///
/// Application-level only — field element encoding is handled by `ValueCodec`.
/// Null/absence is represented by `Option<Value>`, not a variant.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub enum Value {
    /// Unsigned 64-bit integer.
    U64(u64),
    /// Signed 64-bit integer.
    I64(i64),
    /// Boolean value.
    Bool(bool),
    /// 32-byte blob (e.g. hash digest, public key).
    Bytes32([u8; 32]),
}

impl Value {
    /// Check whether this value matches the given type descriptor.
    pub fn matches_type(&self, ty: ValueType) -> bool {
        matches!(
            (self, ty),
            (Value::U64(_), ValueType::U64)
                | (Value::I64(_), ValueType::I64)
                | (Value::Bool(_), ValueType::Bool)
                | (Value::Bytes32(_), ValueType::Bytes32)
        )
    }

    /// Returns the variant name as a static string.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::U64(_) => "U64",
            Value::I64(_) => "I64",
            Value::Bool(_) => "Bool",
            Value::Bytes32(_) => "Bytes32",
        }
    }

    /// Checked addition. Both operands must be the same numeric type.
    pub fn checked_add(&self, rhs: &Value) -> Result<Value, TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => a
                .checked_add(*b)
                .map(Value::U64)
                .ok_or(TabulaError::ArithmeticOverflow),
            (Value::I64(a), Value::I64(b)) => a
                .checked_add(*b)
                .map(Value::I64)
                .ok_or(TabulaError::ArithmeticOverflow),
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
        }
    }

    /// Checked subtraction. Both operands must be the same numeric type.
    pub fn checked_sub(&self, rhs: &Value) -> Result<Value, TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => a
                .checked_sub(*b)
                .map(Value::U64)
                .ok_or(TabulaError::ArithmeticOverflow),
            (Value::I64(a), Value::I64(b)) => a
                .checked_sub(*b)
                .map(Value::I64)
                .ok_or(TabulaError::ArithmeticOverflow),
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
        }
    }

    /// Checked multiplication. Both operands must be the same numeric type.
    pub fn checked_mul(&self, rhs: &Value) -> Result<Value, TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => a
                .checked_mul(*b)
                .map(Value::U64)
                .ok_or(TabulaError::ArithmeticOverflow),
            (Value::I64(a), Value::I64(b)) => a
                .checked_mul(*b)
                .map(Value::I64)
                .ok_or(TabulaError::ArithmeticOverflow),
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
        }
    }

    /// Checked division and modulus. Returns `(quotient, remainder)`.
    pub fn checked_divmod(&self, rhs: &Value) -> Result<(Value, Value), TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => {
                if *b == 0 {
                    return Err(TabulaError::DivisionByZero);
                }
                Ok((Value::U64(a / b), Value::U64(a % b)))
            }
            (Value::I64(a), Value::I64(b)) => {
                if *b == 0 {
                    return Err(TabulaError::DivisionByZero);
                }
                let q = a.checked_div(*b).ok_or(TabulaError::ArithmeticOverflow)?;
                let r = a.checked_rem(*b).ok_or(TabulaError::ArithmeticOverflow)?;
                Ok((Value::I64(q), Value::I64(r)))
            }
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
        }
    }

    /// Compare two values of the same ordered type.
    ///
    /// `Bytes32` is not an ordered type — comparison returns `TypeMismatch`.
    pub fn compare(&self, rhs: &Value) -> Result<Ordering, TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => Ok(a.cmp(b)),
            (Value::I64(a), Value::I64(b)) => Ok(a.cmp(b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
            (Value::Bytes32(_), Value::Bytes32(_)) => Err(TabulaError::TypeMismatch {
                expected: "ordered type (U64, I64, Bool)",
                actual: "Bytes32",
            }),
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::U64(v) => write!(f, "{v}u64"),
            Value::I64(v) => write!(f, "{v}i64"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Bytes32(b) => {
                write!(f, "0x")?;
                for byte in &b[..4] {
                    write!(f, "{byte:02x}")?;
                }
                write!(f, "..{:02x}", b[31])
            }
        }
    }
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueType::U64 => write!(f, "U64"),
            ValueType::I64 => write!(f, "I64"),
            ValueType::Bool => write!(f, "Bool"),
            ValueType::Bytes32 => write!(f, "Bytes32"),
        }
    }
}

/// Return the canonical zero value for a given type.
///
/// Used when a cell is absent (null) — the trace records `(zero_value(T), val_is_null=true)`.
pub fn zero_value(ty: ValueType) -> Value {
    match ty {
        ValueType::U64 => Value::U64(0),
        ValueType::I64 => Value::I64(0),
        ValueType::Bool => Value::Bool(false),
        ValueType::Bytes32 => Value::Bytes32([0; 32]),
    }
}

/// Describes the type of a column or parameter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ValueType {
    /// Unsigned 64-bit integer.
    U64,
    /// Signed 64-bit integer.
    I64,
    /// Boolean.
    Bool,
    /// 32-byte blob.
    Bytes32,
}
