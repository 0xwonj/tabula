//! Expression resolution helpers for the interpreter.
//!
//! Resolves `RowExpr`, `ValueExpr`, and `Predicate` nodes to concrete values
//! using the slot environment and parameter list.

use std::cmp::Ordering;

use tabula_core::error::TabulaError;
use tabula_core::ir::{Predicate, RowExpr, Slot, ValueExpr};
use tabula_core::types::{RowKey, Value};

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
        ValueExpr::Literal(v) => Ok(v.clone()),
        ValueExpr::Slot(s) => get_slot(slots, *s),
        ValueExpr::Param(p) => get_param(params, *p),
    }
}

/// Evaluate a predicate to a boolean.
pub fn evaluate_predicate(
    pred: &Predicate,
    slots: &[Value],
    params: &[Value],
) -> Result<bool, TabulaError> {
    match pred {
        Predicate::Eq(l, r) => {
            let lv = resolve_value_expr(l, slots, params)?;
            let rv = resolve_value_expr(r, slots, params)?;
            Ok(lv == rv)
        }
        Predicate::Lt(l, r) => {
            let lv = resolve_value_expr(l, slots, params)?;
            let rv = resolve_value_expr(r, slots, params)?;
            lv.compare(&rv).map(|ord| ord == Ordering::Less)
        }
        Predicate::Lte(l, r) => {
            let lv = resolve_value_expr(l, slots, params)?;
            let rv = resolve_value_expr(r, slots, params)?;
            lv.compare(&rv).map(|ord| ord != Ordering::Greater)
        }
        Predicate::Gt(l, r) => {
            let lv = resolve_value_expr(l, slots, params)?;
            let rv = resolve_value_expr(r, slots, params)?;
            lv.compare(&rv).map(|ord| ord == Ordering::Greater)
        }
        Predicate::Gte(l, r) => {
            let lv = resolve_value_expr(l, slots, params)?;
            let rv = resolve_value_expr(r, slots, params)?;
            lv.compare(&rv).map(|ord| ord != Ordering::Less)
        }
        Predicate::And(a, b) => {
            let ra = evaluate_predicate(a, slots, params)?;
            let rb = evaluate_predicate(b, slots, params)?;
            Ok(ra && rb)
        }
        Predicate::Or(a, b) => {
            let ra = evaluate_predicate(a, slots, params)?;
            let rb = evaluate_predicate(b, slots, params)?;
            Ok(ra || rb)
        }
        Predicate::Not(p) => {
            let r = evaluate_predicate(p, slots, params)?;
            Ok(!r)
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn get_slot(slots: &[Value], idx: Slot) -> Result<Value, TabulaError> {
    slots
        .get(idx as usize)
        .cloned()
        .ok_or(TabulaError::SlotOutOfBounds {
            index: idx,
            max: slots.len().saturating_sub(1) as u16,
        })
}

fn get_param(params: &[Value], idx: u16) -> Result<Value, TabulaError> {
    params
        .get(idx as usize)
        .cloned()
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
