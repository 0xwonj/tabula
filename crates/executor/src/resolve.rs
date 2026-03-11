//! Expression resolution helpers for the interpreter.
//!
//! Resolves `RowExpr` and `ValueExpr` nodes to concrete values
//! using the slot environment and parameter list.

use tabula_core::error::TabulaError;
use tabula_core::{RowKey, Value};
use tabula_ir::{RowExpr, Slot, ValueExpr};

/// Resolve a `RowExpr` to a concrete `RowKey`.
pub fn resolve_row_expr(
    expr: &RowExpr,
    slots: &[Value],
    params: &[Value],
) -> Result<RowKey, TabulaError> {
    match expr {
        RowExpr::Literal(rk) => Ok(*rk),
        RowExpr::Slot(s) => {
            let v = get_slot(slots, *s)?;
            value_to_row_key(&v)
        }
        RowExpr::Param(p) => {
            let v = get_param(params, *p)?;
            value_to_row_key(&v)
        }
    }
}

/// Resolve a `ValueExpr` to a concrete `Value`.
pub fn resolve_value_expr(
    expr: &ValueExpr,
    slots: &[Value],
    params: &[Value],
) -> Result<Value, TabulaError> {
    match expr {
        ValueExpr::Literal(v) => Ok(*v),
        ValueExpr::Slot(s) => get_slot(slots, *s),
        ValueExpr::Param(p) => get_param(params, *p),
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn get_slot(slots: &[Value], idx: Slot) -> Result<Value, TabulaError> {
    slots
        .get(idx as usize)
        .copied()
        .ok_or(TabulaError::SlotOutOfBounds {
            index: idx,
            max: slots.len().saturating_sub(1) as u16,
        })
}

fn get_param(params: &[Value], idx: u16) -> Result<Value, TabulaError> {
    params
        .get(idx as usize)
        .copied()
        .ok_or(TabulaError::ParamOutOfBounds {
            index: idx,
            max: params.len().saturating_sub(1) as u16,
        })
}

fn value_to_row_key(v: &Value) -> Result<RowKey, TabulaError> {
    match v {
        Value::U64(n) => Ok(RowKey(*n)),
        _ => Err(TabulaError::TypeMismatch {
            expected: "U64",
            actual: v.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── resolve_row_expr ────────────────────────────────────────────

    #[test]
    fn row_expr_literal() {
        let rk = resolve_row_expr(&RowExpr::Literal(RowKey(42)), &[], &[]).unwrap();
        assert_eq!(rk, RowKey(42));
    }

    #[test]
    fn row_expr_slot_u64() {
        let slots = vec![Value::U64(7)];
        let rk = resolve_row_expr(&RowExpr::Slot(0), &slots, &[]).unwrap();
        assert_eq!(rk, RowKey(7));
    }

    #[test]
    fn row_expr_param_u64() {
        let params = vec![Value::U64(99)];
        let rk = resolve_row_expr(&RowExpr::Param(0), &[], &params).unwrap();
        assert_eq!(rk, RowKey(99));
    }

    #[test]
    fn row_expr_slot_non_u64_fails() {
        let slots = vec![Value::Bool(true)];
        let err = resolve_row_expr(&RowExpr::Slot(0), &slots, &[]).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected: "U64",
                ..
            }
        ));
    }

    #[test]
    fn row_expr_slot_out_of_bounds() {
        let err = resolve_row_expr(&RowExpr::Slot(5), &[], &[]).unwrap_err();
        assert!(matches!(err, TabulaError::SlotOutOfBounds { index: 5, .. }));
    }

    #[test]
    fn row_expr_param_out_of_bounds() {
        let err = resolve_row_expr(&RowExpr::Param(3), &[], &[]).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::ParamOutOfBounds { index: 3, .. }
        ));
    }

    // ── resolve_value_expr ──────────────────────────────────────────

    #[test]
    fn value_expr_literal() {
        let v = resolve_value_expr(&ValueExpr::Literal(Value::Bool(true)), &[], &[]).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn value_expr_slot() {
        let slots = vec![Value::I64(-42)];
        let v = resolve_value_expr(&ValueExpr::Slot(0), &slots, &[]).unwrap();
        assert_eq!(v, Value::I64(-42));
    }

    #[test]
    fn value_expr_param() {
        let params = vec![Value::Bytes32([0xab; 32])];
        let v = resolve_value_expr(&ValueExpr::Param(0), &[], &params).unwrap();
        assert_eq!(v, Value::Bytes32([0xab; 32]));
    }

    #[test]
    fn value_expr_slot_out_of_bounds() {
        let err = resolve_value_expr(&ValueExpr::Slot(0), &[], &[]).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::SlotOutOfBounds { index: 0, max: 0 }
        ));
    }

    // ── private helpers ─────────────────────────────────────────────

    #[test]
    fn get_slot_boundary() {
        let slots = vec![Value::U64(10), Value::U64(20)];
        assert_eq!(get_slot(&slots, 0).unwrap(), Value::U64(10));
        assert_eq!(get_slot(&slots, 1).unwrap(), Value::U64(20));
        assert!(get_slot(&slots, 2).is_err());
    }

    #[test]
    fn get_param_boundary() {
        let params = vec![Value::Bool(false)];
        assert_eq!(get_param(&params, 0).unwrap(), Value::Bool(false));
        assert!(get_param(&params, 1).is_err());
    }

    #[test]
    fn get_slot_empty() {
        let err = get_slot(&[], 0).unwrap_err();
        // saturating_sub(1) on len=0 → max=0
        assert!(matches!(
            err,
            TabulaError::SlotOutOfBounds { index: 0, max: 0 }
        ));
    }

    #[test]
    fn value_to_row_key_i64_fails() {
        let err = value_to_row_key(&Value::I64(42)).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected: "U64",
                ..
            }
        ));
    }

    #[test]
    fn value_to_row_key_bytes32_fails() {
        let err = value_to_row_key(&Value::Bytes32([0; 32])).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected: "U64",
                ..
            }
        ));
    }
}
