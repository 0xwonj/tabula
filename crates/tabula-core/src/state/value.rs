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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borsh_round_trip_value_u64() {
        let v = Value::U64(42);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn borsh_round_trip_value_i64() {
        let v = Value::I64(-999);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn borsh_round_trip_value_bool() {
        let v = Value::Bool(true);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn borsh_round_trip_value_bytes32() {
        let v = Value::Bytes32([0xAB; 32]);
        let bytes = borsh::to_vec(&v).unwrap();
        let decoded: Value = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn value_variant_coverage() {
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

    #[test]
    fn type_name() {
        assert_eq!(Value::U64(0).type_name(), "U64");
        assert_eq!(Value::I64(0).type_name(), "I64");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::Bytes32([0; 32]).type_name(), "Bytes32");
    }

    #[test]
    fn checked_add_u64() {
        assert_eq!(
            Value::U64(10).checked_add(&Value::U64(20)).unwrap(),
            Value::U64(30)
        );
    }

    #[test]
    fn checked_add_i64() {
        assert_eq!(
            Value::I64(-5).checked_add(&Value::I64(3)).unwrap(),
            Value::I64(-2)
        );
    }

    #[test]
    fn checked_add_overflow() {
        assert_eq!(
            Value::U64(u64::MAX)
                .checked_add(&Value::U64(1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_add_type_mismatch() {
        assert!(matches!(
            Value::U64(1).checked_add(&Value::I64(1)).unwrap_err(),
            TabulaError::TypeMismatch { .. }
        ));
    }

    #[test]
    fn checked_sub_u64() {
        assert_eq!(
            Value::U64(30).checked_sub(&Value::U64(10)).unwrap(),
            Value::U64(20)
        );
    }

    #[test]
    fn checked_sub_underflow() {
        assert_eq!(
            Value::U64(0).checked_sub(&Value::U64(1)).unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_mul_u64() {
        assert_eq!(
            Value::U64(5).checked_mul(&Value::U64(7)).unwrap(),
            Value::U64(35)
        );
    }

    #[test]
    fn checked_mul_overflow() {
        assert_eq!(
            Value::U64(u64::MAX)
                .checked_mul(&Value::U64(2))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_divmod_u64() {
        let (q, r) = Value::U64(17).checked_divmod(&Value::U64(5)).unwrap();
        assert_eq!(q, Value::U64(3));
        assert_eq!(r, Value::U64(2));
    }

    #[test]
    fn checked_divmod_i64() {
        let (q, r) = Value::I64(-7).checked_divmod(&Value::I64(2)).unwrap();
        assert_eq!(q, Value::I64(-3));
        assert_eq!(r, Value::I64(-1));
    }

    #[test]
    fn checked_divmod_by_zero() {
        assert_eq!(
            Value::U64(10).checked_divmod(&Value::U64(0)).unwrap_err(),
            TabulaError::DivisionByZero
        );
    }

    #[test]
    fn checked_divmod_i64_overflow() {
        assert_eq!(
            Value::I64(i64::MIN)
                .checked_divmod(&Value::I64(-1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn compare_u64() {
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
    fn compare_bool() {
        assert_eq!(
            Value::Bool(false).compare(&Value::Bool(true)).unwrap(),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn compare_type_mismatch() {
        assert!(matches!(
            Value::U64(1).compare(&Value::I64(1)).unwrap_err(),
            TabulaError::TypeMismatch { .. }
        ));
    }

    #[test]
    fn matches_type() {
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
    fn zero_value_per_type() {
        assert_eq!(zero_value(ValueType::U64), Value::U64(0));
        assert_eq!(zero_value(ValueType::I64), Value::I64(0));
        assert_eq!(zero_value(ValueType::Bool), Value::Bool(false));
        assert_eq!(zero_value(ValueType::Bytes32), Value::Bytes32([0; 32]));
    }

    // ── i64 boundary edge cases ──────────────────────────────────────

    #[test]
    fn checked_add_i64_max_overflow() {
        assert_eq!(
            Value::I64(i64::MAX)
                .checked_add(&Value::I64(1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_sub_i64_min_underflow() {
        assert_eq!(
            Value::I64(i64::MIN)
                .checked_sub(&Value::I64(1))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_mul_i64_overflow() {
        assert_eq!(
            Value::I64(i64::MAX)
                .checked_mul(&Value::I64(2))
                .unwrap_err(),
            TabulaError::ArithmeticOverflow
        );
    }

    #[test]
    fn checked_divmod_i64_by_zero() {
        assert_eq!(
            Value::I64(42).checked_divmod(&Value::I64(0)).unwrap_err(),
            TabulaError::DivisionByZero
        );
    }

    // ── i64 comparison ───────────────────────────────────────────────

    #[test]
    fn compare_i64() {
        assert_eq!(
            Value::I64(-10).compare(&Value::I64(10)).unwrap(),
            Ordering::Less
        );
        assert_eq!(
            Value::I64(0).compare(&Value::I64(0)).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            Value::I64(i64::MAX).compare(&Value::I64(i64::MIN)).unwrap(),
            Ordering::Greater
        );
    }

    // ── Bytes32 is not arithmetic / comparable ───────────────────────

    #[test]
    fn bytes32_arithmetic_rejected() {
        let a = Value::Bytes32([1; 32]);
        let b = Value::Bytes32([2; 32]);
        assert!(a.checked_add(&b).is_err());
        assert!(a.checked_sub(&b).is_err());
        assert!(a.checked_mul(&b).is_err());
        assert!(a.checked_divmod(&b).is_err());
    }

    #[test]
    fn bytes32_compare_rejected() {
        let a = Value::Bytes32([1; 32]);
        let b = Value::Bytes32([2; 32]);
        let err = a.compare(&b).unwrap_err();
        match err {
            TabulaError::TypeMismatch { actual, .. } => {
                assert_eq!(actual, "Bytes32");
            }
            _ => panic!("expected TypeMismatch, got {err:?}"),
        }
    }

    // ── Display ──────────────────────────────────────────────────────

    #[test]
    fn display_value() {
        assert_eq!(format!("{}", Value::U64(42)), "42u64");
        assert_eq!(format!("{}", Value::I64(-5)), "-5i64");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert!(format!("{}", Value::Bytes32([0xAB; 32])).starts_with("0xabababab"));
    }

    #[test]
    fn display_value_type() {
        assert_eq!(format!("{}", ValueType::U64), "U64");
        assert_eq!(format!("{}", ValueType::Bytes32), "Bytes32");
    }

    // ── Value is Copy ────────────────────────────────────────────────

    #[test]
    fn value_is_copy() {
        let a = Value::U64(42);
        let b = a; // Copy, not move
        assert_eq!(a, b); // a is still usable
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn u64_add_commutative(a in any::<u64>(), b in any::<u64>()) {
            let va = Value::U64(a);
            let vb = Value::U64(b);
            // If a + b doesn't overflow, result must be commutative
            if let (Ok(ab), Ok(ba)) = (va.checked_add(&vb), vb.checked_add(&va)) {
                prop_assert_eq!(ab, ba);
            }
        }

        #[test]
        fn i64_add_commutative(a in any::<i64>(), b in any::<i64>()) {
            let va = Value::I64(a);
            let vb = Value::I64(b);
            if let (Ok(ab), Ok(ba)) = (va.checked_add(&vb), vb.checked_add(&va)) {
                prop_assert_eq!(ab, ba);
            }
        }

        #[test]
        fn u64_add_zero_identity(a in any::<u64>()) {
            let va = Value::U64(a);
            let zero = Value::U64(0);
            prop_assert_eq!(va.checked_add(&zero).unwrap(), va);
        }

        #[test]
        fn i64_add_zero_identity(a in any::<i64>()) {
            let va = Value::I64(a);
            let zero = Value::I64(0);
            prop_assert_eq!(va.checked_add(&zero).unwrap(), va);
        }

        #[test]
        fn u64_mul_commutative(a in any::<u64>(), b in any::<u64>()) {
            let va = Value::U64(a);
            let vb = Value::U64(b);
            if let (Ok(ab), Ok(ba)) = (va.checked_mul(&vb), vb.checked_mul(&va)) {
                prop_assert_eq!(ab, ba);
            }
        }

        #[test]
        fn u64_divmod_roundtrip(a in any::<u64>(), b in 1..=u64::MAX) {
            let va = Value::U64(a);
            let vb = Value::U64(b);
            let (q, r) = va.checked_divmod(&vb).unwrap();
            if let (Value::U64(qv), Value::U64(rv)) = (q, r) {
                prop_assert_eq!(qv * b + rv, a);
                prop_assert!(rv < b);
            } else {
                prop_assert!(false, "expected U64 results");
            }
        }

        #[test]
        fn u64_sub_inverse_of_add(a in any::<u64>(), b in any::<u64>()) {
            let va = Value::U64(a);
            let vb = Value::U64(b);
            if let Ok(sum) = va.checked_add(&vb) {
                prop_assert_eq!(sum.checked_sub(&vb).unwrap(), va);
            }
        }

        #[test]
        fn borsh_round_trip_any_u64(n in any::<u64>()) {
            let v = Value::U64(n);
            let bytes = borsh::to_vec(&v).unwrap();
            let decoded: Value = borsh::from_slice(&bytes).unwrap();
            prop_assert_eq!(v, decoded);
        }

        #[test]
        fn borsh_round_trip_any_i64(n in any::<i64>()) {
            let v = Value::I64(n);
            let bytes = borsh::to_vec(&v).unwrap();
            let decoded: Value = borsh::from_slice(&bytes).unwrap();
            prop_assert_eq!(v, decoded);
        }

        #[test]
        fn compare_consistent_with_std(a in any::<u64>(), b in any::<u64>()) {
            let va = Value::U64(a);
            let vb = Value::U64(b);
            prop_assert_eq!(va.compare(&vb).unwrap(), a.cmp(&b));
        }

        #[test]
        fn compare_i64_consistent_with_std(a in any::<i64>(), b in any::<i64>()) {
            let va = Value::I64(a);
            let vb = Value::I64(b);
            prop_assert_eq!(va.compare(&vb).unwrap(), a.cmp(&b));
        }
    }
}
