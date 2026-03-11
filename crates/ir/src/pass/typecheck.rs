//! IR type inference and SSA validation.
//!
//! Walks the instruction body, checks param/slot bounds, enforces SSA
//! (each slot assigned at most once), and infers slot types from schemas.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId, TableSchema, ValueType};

use crate::{Instruction, RowExpr, Slot, TxTypeDef, ValueExpr};

/// Inferred type information for a transaction body.
///
/// Computed at registration time from IR + `param_schema`.
/// `slot_types[i]` is `None` when the type cannot be determined statically
/// (e.g. `Read` result without table schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyTypeInfo {
    /// Inferred type per slot. `None` = unknown.
    pub slot_types: Vec<Option<ValueType>>,
    /// Parameter types (copied from `param_schema` for convenience).
    pub param_types: Vec<ValueType>,
    /// Highest slot index assigned in the body.
    pub max_slot: Option<Slot>,
}

/// Validate references and infer slot types for a tx body.
pub fn check(
    def: &TxTypeDef,
    schemas: &BTreeMap<TableId, TableSchema>,
) -> Result<BodyTypeInfo, TabulaError> {
    let param_count = def.param_schema.len() as u16;
    let param_types: Vec<ValueType> = def.param_schema.iter().map(|p| p.value_type).collect();

    let mut slot_types: Vec<Option<ValueType>> = Vec::new();
    let mut assigned_at: Vec<Option<usize>> = Vec::new();
    let mut max_slot: Option<Slot> = None;

    let assign_slot = |slot: Slot,
                       ty: Option<ValueType>,
                       instr_idx: usize,
                       slot_types: &mut Vec<Option<ValueType>>,
                       assigned_at: &mut Vec<Option<usize>>,
                       max_slot: &mut Option<Slot>|
     -> Result<(), TabulaError> {
        let idx = slot as usize;
        if idx >= slot_types.len() {
            slot_types.resize(idx + 1, None);
            assigned_at.resize(idx + 1, None);
        }
        if let Some(prev) = assigned_at[idx] {
            return Err(TabulaError::InvalidIr(format!(
                "instruction {instr_idx}: slot {slot} already assigned at instruction {prev} (SSA violation)"
            )));
        }
        slot_types[idx] = ty;
        assigned_at[idx] = Some(instr_idx);
        *max_slot = Some(max_slot.map_or(slot, |m: Slot| m.max(slot)));
        Ok(())
    };

    for (i, instr) in def.body.iter().enumerate() {
        match instr {
            Instruction::Read {
                dst_val,
                dst_is_null,
                table,
                col,
                row,
            } => {
                check_row_expr(row, param_count, &param_types, &slot_types, i)?;
                let ty = schema_col_type(schemas, *table, *col);
                assign_slot(
                    *dst_val,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
                assign_slot(
                    *dst_is_null,
                    Some(ValueType::Bool),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Write {
                table,
                col,
                row,
                src_val,
                src_is_null,
            } => {
                check_row_expr(row, param_count, &param_types, &slot_types, i)?;
                check_value_expr(src_val, param_count, &slot_types, i)?;
                check_value_expr(src_is_null, param_count, &slot_types, i)?;
                if let Some(expected) = schema_col_type(schemas, *table, *col)
                    && let Some(actual) = expr_type(src_val, &param_types, &slot_types)
                    && actual != expected
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: write to table {} col {} expects {expected:?} but got {actual:?}",
                        table.0, col.0
                    )));
                }
                if let Some(is_null_ty) = expr_type(src_is_null, &param_types, &slot_types)
                    && is_null_ty != ValueType::Bool
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: write src_is_null must be Bool, got {is_null_ty:?}"
                    )));
                }
            }

            Instruction::Lookup {
                dst,
                static_table,
                col,
                row,
            } => {
                check_row_expr(row, param_count, &param_types, &slot_types, i)?;
                let ty = schema_col_type(schemas, *static_table, *col);
                assign_slot(
                    *dst,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Arith { dst, lhs, rhs, .. } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(
                    *dst,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty = infer_numeric_result(lhs, rhs, &param_types, &slot_types, i)?;
                assign_slot(
                    *dst_q,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
                assign_slot(
                    *dst_r,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Cmp { dst, lhs, rhs, .. } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                assign_slot(
                    *dst,
                    Some(ValueType::Bool),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Not { dst, src } => {
                check_value_expr(src, param_count, &slot_types, i)?;
                if let Some(ty) = expr_type(src, &param_types, &slot_types)
                    && ty != ValueType::Bool
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: Not operand must be Bool, got {ty:?}"
                    )));
                }
                assign_slot(
                    *dst,
                    Some(ValueType::Bool),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::And { dst, lhs, rhs } | Instruction::Or { dst, lhs, rhs } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                for (label, operand) in [("lhs", lhs), ("rhs", rhs)] {
                    if let Some(ty) = expr_type(operand, &param_types, &slot_types)
                        && ty != ValueType::Bool
                    {
                        return Err(TabulaError::InvalidIr(format!(
                            "instruction {i}: And/Or {label} must be Bool, got {ty:?}"
                        )));
                    }
                }
                assign_slot(
                    *dst,
                    Some(ValueType::Bool),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Assert { cond } => {
                check_value_expr(cond, param_count, &slot_types, i)?;
                if let Some(ty) = expr_type(cond, &param_types, &slot_types)
                    && ty != ValueType::Bool
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: assert condition must be Bool, got {ty:?}"
                    )));
                }
            }

            Instruction::Hash { dst, inputs } => {
                for input in inputs {
                    check_value_expr(input, param_count, &slot_types, i)?;
                }
                assign_slot(
                    *dst,
                    Some(ValueType::Bytes32),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => {
                check_value_expr(cond, param_count, &slot_types, i)?;
                check_value_expr(if_true, param_count, &slot_types, i)?;
                check_value_expr(if_false, param_count, &slot_types, i)?;
                if let Some(ct) = expr_type(cond, &param_types, &slot_types)
                    && ct != ValueType::Bool
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: select condition must be Bool, got {ct:?}"
                    )));
                }
                let tt = expr_type(if_true, &param_types, &slot_types);
                let ft = expr_type(if_false, &param_types, &slot_types);
                let ty = match (tt, ft) {
                    (Some(t), Some(f)) if t != f => {
                        return Err(TabulaError::InvalidIr(format!(
                            "instruction {i}: select branches type mismatch: {t:?} vs {f:?}"
                        )));
                    }
                    (Some(t), _) => Some(t),
                    (_, Some(f)) => Some(f),
                    (None, None) => None,
                };
                assign_slot(
                    *dst,
                    ty,
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Emit { data, .. } => {
                for d in data {
                    check_value_expr(d, param_count, &slot_types, i)?;
                }
            }
        }
    }

    Ok(BodyTypeInfo {
        slot_types,
        param_types,
        max_slot,
    })
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn check_row_expr(
    expr: &RowExpr,
    param_count: u16,
    param_types: &[ValueType],
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match expr {
        RowExpr::Literal(_) => Ok(()),
        RowExpr::Param(idx) => {
            check_param_idx(*idx, param_count, instr_idx)?;
            if let Some(ty) = param_types.get(*idx as usize)
                && *ty != ValueType::U64
            {
                return Err(TabulaError::InvalidIr(format!(
                    "instruction {instr_idx}: row expression param {idx} must be U64, got {ty:?}"
                )));
            }
            Ok(())
        }
        RowExpr::Slot(idx) => {
            check_slot_defined(*idx, slot_types, instr_idx)?;
            if let Some(Some(ty)) = slot_types.get(*idx as usize)
                && *ty != ValueType::U64
            {
                return Err(TabulaError::InvalidIr(format!(
                    "instruction {instr_idx}: row expression slot {idx} must be U64, got {ty:?}"
                )));
            }
            Ok(())
        }
    }
}

fn check_value_expr(
    expr: &ValueExpr,
    param_count: u16,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match expr {
        ValueExpr::Literal(_) => Ok(()),
        ValueExpr::Param(idx) => check_param_idx(*idx, param_count, instr_idx),
        ValueExpr::Slot(idx) => check_slot_defined(*idx, slot_types, instr_idx),
    }
}

fn check_param_idx(idx: u16, param_count: u16, instr_idx: usize) -> Result<(), TabulaError> {
    if idx >= param_count {
        return Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: param index {idx} out of bounds (schema has {param_count} params)"
        )));
    }
    Ok(())
}

fn check_slot_defined(
    idx: Slot,
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let i = idx as usize;
    if i >= slot_types.len() {
        return Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: slot {idx} read before assignment"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Type inference helpers
// ---------------------------------------------------------------------------

fn schema_col_type(
    schemas: &BTreeMap<TableId, TableSchema>,
    table: TableId,
    col: ColId,
) -> Option<ValueType> {
    schemas
        .get(&table)
        .and_then(|s| s.columns.iter().find(|c| c.id == col))
        .map(|c| c.value_type)
}

fn infer_numeric_result(
    lhs: &ValueExpr,
    rhs: &ValueExpr,
    param_types: &[ValueType],
    slot_types: &[Option<ValueType>],
    instr_idx: usize,
) -> Result<Option<ValueType>, TabulaError> {
    let lt = expr_type(lhs, param_types, slot_types);
    let rt = expr_type(rhs, param_types, slot_types);
    match (lt, rt) {
        (Some(l), Some(r)) if l != r => Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: operand type mismatch: {l:?} vs {r:?}"
        ))),
        (Some(l), _) => Ok(Some(l)),
        (_, Some(r)) => Ok(Some(r)),
        (None, None) => Ok(None),
    }
}

fn expr_type(
    expr: &ValueExpr,
    param_types: &[ValueType],
    slot_types: &[Option<ValueType>],
) -> Option<ValueType> {
    match expr {
        ValueExpr::Literal(v) => value_to_type(v),
        ValueExpr::Param(idx) => param_types.get(*idx as usize).copied(),
        ValueExpr::Slot(idx) => slot_types.get(*idx as usize).copied().flatten(),
    }
}

fn value_to_type(v: &tabula_core::Value) -> Option<ValueType> {
    match v {
        tabula_core::Value::U64(_) => Some(ValueType::U64),
        tabula_core::Value::I64(_) => Some(ValueType::I64),
        tabula_core::Value::Bool(_) => Some(ValueType::Bool),
        tabula_core::Value::Bytes32(_) => Some(ValueType::Bytes32),
    }
}
