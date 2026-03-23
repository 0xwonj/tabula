//! Expression resolution helpers for the interpreter.
//!
//! Resolves `RowExpr` and `ValueExpr` nodes to concrete values
//! using the slot environment and parameter list.

use tabula_core::RowKey;
use tabula_core::error::TabulaError;
use tabula_ir::{RowExpr, Slot, ValueExpr};
use tabula_types::{TypeRuntimeRegistry, TypedValue, typed_row_key};

/// Resolve a `RowExpr` to a concrete `RowKey`.
pub fn resolve_row_expr(
    expr: &RowExpr,
    slots: &[TypedValue],
    params: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    match expr {
        RowExpr::Literal(rk) => Ok(*rk),
        RowExpr::Slot(s) => {
            let v = get_slot(slots, *s)?;
            typed_row_key(&v, type_runtimes)
        }
        RowExpr::Param(p) => {
            let v = get_param(params, *p)?;
            typed_row_key(&v, type_runtimes)
        }
    }
}

/// Resolve a `ValueExpr` to a concrete typed runtime value.
pub fn resolve_value_expr(
    expr: &ValueExpr,
    slots: &[TypedValue],
    params: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<TypedValue, TabulaError> {
    match expr {
        ValueExpr::Literal(v) => type_runtimes.decode_portable(v),
        ValueExpr::Slot(s) => get_slot(slots, *s),
        ValueExpr::Param(p) => get_param(params, *p),
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn get_slot(slots: &[TypedValue], idx: Slot) -> Result<TypedValue, TabulaError> {
    slots
        .get(idx as usize)
        .cloned()
        .ok_or(TabulaError::SlotOutOfBounds {
            index: idx,
            max: slots.len().saturating_sub(1) as u16,
        })
}

fn get_param(params: &[TypedValue], idx: u16) -> Result<TypedValue, TabulaError> {
    params
        .get(idx as usize)
        .cloned()
        .ok_or(TabulaError::ParamOutOfBounds {
            index: idx,
            max: params.len().saturating_sub(1) as u16,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_types::{
        TypeRuntimeRegistry, bool_portable, bool_typed, bytes32_typed, i64_typed, u64_typed,
    };

    fn runtimes() -> TypeRuntimeRegistry {
        TypeRuntimeRegistry::seeded().expect("seeded type runtimes")
    }

    // ── resolve_row_expr ────────────────────────────────────────────

    #[test]
    fn row_expr_literal() {
        let type_runtimes = runtimes();
        let rk = resolve_row_expr(&RowExpr::Literal(RowKey(42)), &[], &[], &type_runtimes).unwrap();
        assert_eq!(rk, RowKey(42));
    }

    #[test]
    fn row_expr_slot_u64() {
        let type_runtimes = runtimes();
        let slots = vec![u64_typed(7)];
        let rk = resolve_row_expr(&RowExpr::Slot(0), &slots, &[], &type_runtimes).unwrap();
        assert_eq!(rk, RowKey(7));
    }

    #[test]
    fn row_expr_param_u64() {
        let type_runtimes = runtimes();
        let params = vec![u64_typed(99)];
        let rk = resolve_row_expr(&RowExpr::Param(0), &[], &params, &type_runtimes).unwrap();
        assert_eq!(rk, RowKey(99));
    }

    #[test]
    fn row_expr_slot_non_u64_fails() {
        let type_runtimes = runtimes();
        let slots = vec![bool_typed(true)];
        let err = resolve_row_expr(&RowExpr::Slot(0), &slots, &[], &type_runtimes).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected,
                ..
            } if expected == "UnsignedInteger(64)"
        ));
    }

    #[test]
    fn row_expr_slot_out_of_bounds() {
        let type_runtimes = runtimes();
        let err = resolve_row_expr(&RowExpr::Slot(5), &[], &[], &type_runtimes).unwrap_err();
        assert!(matches!(err, TabulaError::SlotOutOfBounds { index: 5, .. }));
    }

    #[test]
    fn row_expr_param_out_of_bounds() {
        let type_runtimes = runtimes();
        let err = resolve_row_expr(&RowExpr::Param(3), &[], &[], &type_runtimes).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::ParamOutOfBounds { index: 3, .. }
        ));
    }

    // ── resolve_value_expr ──────────────────────────────────────────

    #[test]
    fn value_expr_literal() {
        let type_runtimes = runtimes();
        let v = resolve_value_expr(
            &ValueExpr::Literal(bool_portable(true)),
            &[],
            &[],
            &type_runtimes,
        )
        .unwrap();
        assert_eq!(v, bool_typed(true));
    }

    #[test]
    fn value_expr_slot() {
        let type_runtimes = runtimes();
        let slots = vec![i64_typed(-42)];
        let v = resolve_value_expr(&ValueExpr::Slot(0), &slots, &[], &type_runtimes).unwrap();
        assert_eq!(v, i64_typed(-42));
    }

    #[test]
    fn value_expr_param() {
        let type_runtimes = runtimes();
        let params = vec![bytes32_typed([0xab; 32])];
        let v = resolve_value_expr(&ValueExpr::Param(0), &[], &params, &type_runtimes).unwrap();
        assert_eq!(v, bytes32_typed([0xab; 32]));
    }

    #[test]
    fn value_expr_slot_out_of_bounds() {
        let type_runtimes = runtimes();
        let err = resolve_value_expr(&ValueExpr::Slot(0), &[], &[], &type_runtimes).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::SlotOutOfBounds { index: 0, max: 0 }
        ));
    }

    // ── private helpers ─────────────────────────────────────────────

    #[test]
    fn get_slot_boundary() {
        let slots = vec![u64_typed(10), u64_typed(20)];
        assert_eq!(get_slot(&slots, 0).unwrap(), u64_typed(10));
        assert_eq!(get_slot(&slots, 1).unwrap(), u64_typed(20));
        assert!(get_slot(&slots, 2).is_err());
    }

    #[test]
    fn get_param_boundary() {
        let params = vec![bool_typed(false)];
        assert_eq!(get_param(&params, 0).unwrap(), bool_typed(false));
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
        let type_runtimes = runtimes();
        let err = typed_row_key(&i64_typed(42), &type_runtimes).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected,
                ..
            } if expected == "UnsignedInteger(64)"
        ));
    }

    #[test]
    fn value_to_row_key_bytes32_fails() {
        let type_runtimes = runtimes();
        let err = typed_row_key(&bytes32_typed([0; 32]), &type_runtimes).unwrap_err();
        assert!(matches!(
            err,
            TabulaError::TypeMismatch {
                expected,
                ..
            } if expected == "UnsignedInteger(64)"
        ));
    }
}
