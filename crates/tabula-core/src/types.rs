//! Primitive identifiers and value types for the Tabula kernel.

use std::cmp::Ordering;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::error::TabulaError;

/// Identifies a table in the state.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TableId(pub u32);

/// Identifies a column within a table.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ColId(pub u16);

/// Row key. Dense integer keys for kernel v1.0.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct RowKey(pub u64);

/// A fully-qualified cell address.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CellKey {
    /// The table containing this cell.
    pub table: TableId,
    /// The column within the table.
    pub col: ColId,
    /// The row within the table.
    pub row: RowKey,
}

/// A typed value stored in a table cell.
///
/// Application-level only — field element encoding is handled by `ValueCodec`.
/// Null/absence is represented by `Option<Value>`, not a variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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
    pub fn compare(&self, rhs: &Value) -> Result<Ordering, TabulaError> {
        match (self, rhs) {
            (Value::U64(a), Value::U64(b)) => Ok(a.cmp(b)),
            (Value::I64(a), Value::I64(b)) => Ok(a.cmp(b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
            _ => Err(TabulaError::TypeMismatch {
                expected: self.type_name(),
                actual: rhs.type_name(),
            }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_borsh_round_trip_value_u64() {
        let v = Value::U64(42);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_borsh_round_trip_value_i64() {
        let v = Value::I64(-999);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_borsh_round_trip_value_bool() {
        let v = Value::Bool(true);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_borsh_round_trip_value_bytes32() {
        let v = Value::Bytes32([0xAB; 32]);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn test_cellkey_ordering() {
        // Canonical sort order: (table, col, row)
        let a = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        let b = CellKey {
            table: TableId(1),
            col: ColId(1),
            row: RowKey(0),
        };
        let c = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(1),
        };
        let d = CellKey {
            table: TableId(2),
            col: ColId(0),
            row: RowKey(0),
        };

        assert!(a < b, "same table, col 0 < col 1");
        assert!(a < c, "same table+col, row 0 < row 1");
        assert!(c < d, "table 1 < table 2");
        // Verify col sorts before row (canonical (t,c,r) order)
        assert!(
            c < b,
            "col 0 row 1 < col 1 row 0: col takes priority over row"
        );
    }

    #[test]
    fn test_borsh_round_trip_cellkey() {
        let ck = CellKey {
            table: TableId(5),
            col: ColId(3),
            row: RowKey(100),
        };
        let bytes = borsh::to_vec(&ck).unwrap();
        let decoded: CellKey = borsh::from_slice(&bytes).unwrap();
        assert_eq!(ck, decoded);
    }

    #[test]
    fn test_value_variant_coverage() {
        // Ensure all four variants are distinct under PartialEq.
        let variants = [
            Value::U64(0),
            Value::I64(0),
            Value::Bool(false),
            Value::Bytes32([0; 32]),
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // --- Value arithmetic method tests ---

    #[test]
    fn test_type_name() {
        assert_eq!(Value::U64(0).type_name(), "U64");
        assert_eq!(Value::I64(0).type_name(), "I64");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::Bytes32([0; 32]).type_name(), "Bytes32");
    }

    #[test]
    fn test_checked_add_u64() {
        assert_eq!(
            Value::U64(10).checked_add(&Value::U64(20)).unwrap(),
            Value::U64(30)
        );
    }

    #[test]
    fn test_checked_add_i64() {
        assert_eq!(
            Value::I64(-5).checked_add(&Value::I64(3)).unwrap(),
            Value::I64(-2)
        );
    }

    #[test]
    fn test_checked_add_overflow() {
        assert_eq!(
            Value::U64(u64::MAX)
                .checked_add(&Value::U64(1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn test_checked_add_type_mismatch() {
        assert!(matches!(
            Value::U64(1).checked_add(&Value::I64(1)).unwrap_err(),
            TabulaError::TypeMismatch { .. }
        ));
    }

    #[test]
    fn test_checked_sub_u64() {
        assert_eq!(
            Value::U64(30).checked_sub(&Value::U64(10)).unwrap(),
            Value::U64(20)
        );
    }

    #[test]
    fn test_checked_sub_underflow() {
        assert_eq!(
            Value::U64(0).checked_sub(&Value::U64(1)).unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn test_checked_mul_u64() {
        assert_eq!(
            Value::U64(5).checked_mul(&Value::U64(7)).unwrap(),
            Value::U64(35)
        );
    }

    #[test]
    fn test_checked_mul_overflow() {
        assert_eq!(
            Value::U64(u64::MAX)
                .checked_mul(&Value::U64(2))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn test_checked_divmod_u64() {
        let (q, r) = Value::U64(17).checked_divmod(&Value::U64(5)).unwrap();
        assert_eq!(q, Value::U64(3));
        assert_eq!(r, Value::U64(2));
    }

    #[test]
    fn test_checked_divmod_i64() {
        let (q, r) = Value::I64(-7).checked_divmod(&Value::I64(2)).unwrap();
        assert_eq!(q, Value::I64(-3));
        assert_eq!(r, Value::I64(-1));
    }

    #[test]
    fn test_checked_divmod_by_zero() {
        assert_eq!(
            Value::U64(10).checked_divmod(&Value::U64(0)).unwrap_err(),
            TabulaError::DivisionByZero
        );
    }

    #[test]
    fn test_checked_divmod_i64_overflow() {
        assert_eq!(
            Value::I64(i64::MIN)
                .checked_divmod(&Value::I64(-1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn test_compare_u64() {
        assert_eq!(
            Value::U64(1).compare(&Value::U64(2)).unwrap(),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            Value::U64(5).compare(&Value::U64(5)).unwrap(),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_compare_bool() {
        assert_eq!(
            Value::Bool(false).compare(&Value::Bool(true)).unwrap(),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_compare_type_mismatch() {
        assert!(matches!(
            Value::U64(1).compare(&Value::I64(1)).unwrap_err(),
            TabulaError::TypeMismatch { .. }
        ));
    }

    #[test]
    fn test_matches_type() {
        assert!(Value::U64(0).matches_type(ValueType::U64));
        assert!(Value::I64(0).matches_type(ValueType::I64));
        assert!(Value::Bool(true).matches_type(ValueType::Bool));
        assert!(Value::Bytes32([0; 32]).matches_type(ValueType::Bytes32));

        assert!(!Value::U64(0).matches_type(ValueType::I64));
        assert!(!Value::I64(0).matches_type(ValueType::U64));
        assert!(!Value::Bool(true).matches_type(ValueType::U64));
        assert!(!Value::Bool(true).matches_type(ValueType::Bytes32));
    }

    #[test]
    fn test_zero_value_per_type() {
        assert_eq!(zero_value(ValueType::U64), Value::U64(0));
        assert_eq!(zero_value(ValueType::I64), Value::I64(0));
        assert_eq!(zero_value(ValueType::Bool), Value::Bool(false));
        assert_eq!(zero_value(ValueType::Bytes32), Value::Bytes32([0; 32]));
    }
}
