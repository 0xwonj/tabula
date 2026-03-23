//! IR type inference and SSA validation.
//!
//! Walks the instruction body, checks param/slot bounds, enforces SSA
//! (each slot assigned at most once), and infers slot types from schemas and
//! the canonical profile catalog.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId, TableSchema, TypeId};
use tabula_profile::{
    ProfileCatalog, TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID, TypeCapabilities,
};

use crate::{
    CmpOp, Instruction, PrecompileId, PrecompileSignature, RowExpr, Slot, TxTypeDef, ValueExpr,
};

/// Inferred type information for a transaction body.
///
/// Computed at registration time from IR + `param_schema`.
/// `slot_types[i]` is `None` when the type cannot be determined statically
/// (e.g. precompile result slots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyTypeInfo {
    /// Inferred type per slot. `None` = unknown.
    pub slot_types: Vec<Option<TypeId>>,
    /// Parameter types (copied from `param_schema` for convenience).
    pub param_types: Vec<TypeId>,
    /// Highest slot index assigned in the body.
    pub max_slot: Option<Slot>,
}

/// Validate references and infer slot types for a tx body.
pub fn check(
    def: &TxTypeDef,
    schemas: &BTreeMap<TableId, TableSchema>,
    profile_catalog: &ProfileCatalog,
    precompiles: &BTreeMap<PrecompileId, PrecompileSignature>,
) -> Result<BodyTypeInfo, TabulaError> {
    let param_count = def.param_schema.len() as u16;
    let param_types: Vec<TypeId> = def.param_schema.iter().map(|p| p.type_id).collect();

    for param in &def.param_schema {
        let _ = type_capabilities(profile_catalog, param.type_id)?;
    }

    let mut slot_types: Vec<Option<TypeId>> = Vec::new();
    let mut assigned_at: Vec<Option<usize>> = Vec::new();
    let mut max_slot: Option<Slot> = None;

    let assign_slot = |slot: Slot,
                       ty: Option<TypeId>,
                       instr_idx: usize,
                       slot_types: &mut Vec<Option<TypeId>>,
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
                let ty = schema_col_type(profile_catalog, schemas, *table, *col)?;
                assign_slot(
                    *dst_val,
                    Some(ty),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
                assign_slot(
                    *dst_is_null,
                    Some(TYPE_BOOL_ID),
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
                let expected = schema_col_type(profile_catalog, schemas, *table, *col)?;
                if let Some(actual) = expr_type(src_val, &param_types, &slot_types)
                    && actual != expected
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: write to table {} col {} expects type_id {} but got {}",
                        table.0, col.0, expected.0, actual.0
                    )));
                }
                if let Some(is_null_ty) = expr_type(src_is_null, &param_types, &slot_types)
                    && is_null_ty != TYPE_BOOL_ID
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: write src_is_null must be Bool, got type_id {}",
                        is_null_ty.0
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
                let ty = schema_col_type(profile_catalog, schemas, *static_table, *col)?;
                assign_slot(
                    *dst,
                    Some(ty),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Arith { dst, lhs, rhs, .. } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                let ty =
                    infer_numeric_result(profile_catalog, lhs, rhs, &param_types, &slot_types, i)?;
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
                let ty =
                    infer_numeric_result(profile_catalog, lhs, rhs, &param_types, &slot_types, i)?;
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

            Instruction::Cmp { dst, lhs, rhs, op } => {
                check_value_expr(lhs, param_count, &slot_types, i)?;
                check_value_expr(rhs, param_count, &slot_types, i)?;
                validate_cmp_operands(
                    profile_catalog,
                    *op,
                    lhs,
                    rhs,
                    &param_types,
                    &slot_types,
                    i,
                )?;
                assign_slot(
                    *dst,
                    Some(TYPE_BOOL_ID),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Not { dst, src } => {
                check_value_expr(src, param_count, &slot_types, i)?;
                if let Some(ty) = expr_type(src, &param_types, &slot_types)
                    && ty != TYPE_BOOL_ID
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: Not operand must be Bool, got type_id {}",
                        ty.0
                    )));
                }
                assign_slot(
                    *dst,
                    Some(TYPE_BOOL_ID),
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
                        && ty != TYPE_BOOL_ID
                    {
                        return Err(TabulaError::InvalidIr(format!(
                            "instruction {i}: And/Or {label} must be Bool, got type_id {}",
                            ty.0
                        )));
                    }
                }
                assign_slot(
                    *dst,
                    Some(TYPE_BOOL_ID),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }

            Instruction::Assert { cond } => {
                check_value_expr(cond, param_count, &slot_types, i)?;
                if let Some(ty) = expr_type(cond, &param_types, &slot_types)
                    && ty != TYPE_BOOL_ID
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: assert condition must be Bool, got type_id {}",
                        ty.0
                    )));
                }
            }

            Instruction::Hash { dst, inputs } => {
                for input in inputs {
                    check_value_expr(input, param_count, &slot_types, i)?;
                }
                assign_slot(
                    *dst,
                    Some(TYPE_BYTES32_ID),
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
                    && ct != TYPE_BOOL_ID
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: select condition must be Bool, got type_id {}",
                        ct.0
                    )));
                }
                let tt = expr_type(if_true, &param_types, &slot_types);
                let ft = expr_type(if_false, &param_types, &slot_types);
                let ty = match (tt, ft) {
                    (Some(t), Some(f)) if t != f => {
                        return Err(TabulaError::InvalidIr(format!(
                            "instruction {i}: select branches type mismatch: {} vs {}",
                            t.0, f.0
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

            Instruction::Precompile {
                id,
                dst_slots,
                inputs,
            } => {
                let Some(signature) = precompiles.get(id) else {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: precompile 0x{:04x} has no sealed signature",
                        id.0
                    )));
                };
                if inputs.len() != signature.inputs.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: precompile 0x{:04x} expects {} inputs but IR provides {}",
                        id.0,
                        signature.inputs.len(),
                        inputs.len()
                    )));
                }
                if dst_slots.len() != signature.outputs.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "instruction {i}: precompile 0x{:04x} expects {} outputs but IR declares {} dst slots",
                        id.0,
                        signature.outputs.len(),
                        dst_slots.len()
                    )));
                }
                for input in inputs {
                    check_value_expr(input, param_count, &slot_types, i)?;
                }
                for (input, expected) in inputs.iter().zip(&signature.inputs) {
                    if let Some(actual) = expr_type(input, &param_types, &slot_types)
                        && actual != expected.type_id
                    {
                        return Err(TabulaError::InvalidIr(format!(
                            "instruction {i}: precompile 0x{:04x} input expects type_id {} but got {}",
                            id.0, expected.type_id.0, actual.0
                        )));
                    }
                }
                for (dst, output) in dst_slots.iter().zip(&signature.outputs) {
                    assign_slot(
                        *dst,
                        Some(output.type_id),
                        i,
                        &mut slot_types,
                        &mut assigned_at,
                        &mut max_slot,
                    )?;
                }
            }

            Instruction::PropertyRead {
                dst_val,
                dst_key,
                dst_is_null,
                table,
                col,
                ..
            } => {
                let ty = schema_col_type(profile_catalog, schemas, *table, *col)?;
                assign_slot(
                    *dst_val,
                    Some(ty),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
                assign_slot(
                    *dst_key,
                    Some(TYPE_U64_ID),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
                assign_slot(
                    *dst_is_null,
                    Some(TYPE_BOOL_ID),
                    i,
                    &mut slot_types,
                    &mut assigned_at,
                    &mut max_slot,
                )?;
            }
        }
    }

    Ok(BodyTypeInfo {
        slot_types,
        param_types,
        max_slot,
    })
}

fn check_row_expr(
    expr: &RowExpr,
    param_count: u16,
    param_types: &[TypeId],
    slot_types: &[Option<TypeId>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    match expr {
        RowExpr::Literal(_) => Ok(()),
        RowExpr::Param(idx) => {
            check_param_idx(*idx, param_count, instr_idx)?;
            if let Some(ty) = param_types.get(*idx as usize)
                && *ty != TYPE_U64_ID
            {
                return Err(TabulaError::InvalidIr(format!(
                    "instruction {instr_idx}: row expression param {idx} must be U64, got type_id {}",
                    ty.0
                )));
            }
            Ok(())
        }
        RowExpr::Slot(idx) => {
            check_slot_defined(*idx, slot_types, instr_idx)?;
            if let Some(Some(ty)) = slot_types.get(*idx as usize)
                && *ty != TYPE_U64_ID
            {
                return Err(TabulaError::InvalidIr(format!(
                    "instruction {instr_idx}: row expression slot {idx} must be U64, got type_id {}",
                    ty.0
                )));
            }
            Ok(())
        }
    }
}

fn check_value_expr(
    expr: &ValueExpr,
    param_count: u16,
    slot_types: &[Option<TypeId>],
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
    slot_types: &[Option<TypeId>],
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

fn schema_col_type(
    profile_catalog: &ProfileCatalog,
    schemas: &BTreeMap<TableId, TableSchema>,
    table: TableId,
    col: ColId,
) -> Result<TypeId, TabulaError> {
    let Some(column) = schemas
        .get(&table)
        .and_then(|schema| schema.columns.iter().find(|column| column.id == col))
    else {
        return Err(TabulaError::InvalidIr(format!(
            "schema is missing table {} col {}",
            table.0, col.0
        )));
    };
    profile_catalog
        .resolve_column_profile(column.column_profile_id)
        .map(|resolved| resolved.type_descriptor.type_id)
        .map_err(|err| {
            TabulaError::InvalidIr(format!(
                "schema column table {} col {} references invalid column profile {}: {err}",
                table.0, col.0, column.column_profile_id.0
            ))
        })
}

fn infer_numeric_result(
    profile_catalog: &ProfileCatalog,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
    param_types: &[TypeId],
    slot_types: &[Option<TypeId>],
    instr_idx: usize,
) -> Result<Option<TypeId>, TabulaError> {
    let lt = expr_type(lhs, param_types, slot_types);
    let rt = expr_type(rhs, param_types, slot_types);
    match (lt, rt) {
        (Some(l), Some(r)) if l != r => Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: operand type mismatch: {} vs {}",
            l.0, r.0
        ))),
        (Some(l), _) => {
            ensure_arithmetic_type(profile_catalog, l, instr_idx)?;
            Ok(Some(l))
        }
        (_, Some(r)) => {
            ensure_arithmetic_type(profile_catalog, r, instr_idx)?;
            Ok(Some(r))
        }
        (None, None) => Ok(None),
    }
}

fn validate_cmp_operands(
    profile_catalog: &ProfileCatalog,
    op: CmpOp,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
    param_types: &[TypeId],
    slot_types: &[Option<TypeId>],
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let lt = expr_type(lhs, param_types, slot_types);
    let rt = expr_type(rhs, param_types, slot_types);
    let ty = match (lt, rt) {
        (Some(l), Some(r)) if l != r => {
            return Err(TabulaError::InvalidIr(format!(
                "instruction {instr_idx}: comparison operand type mismatch: {} vs {}",
                l.0, r.0
            )));
        }
        (Some(l), _) => l,
        (_, Some(r)) => r,
        (None, None) => return Ok(()),
    };
    let capabilities = type_capabilities(profile_catalog, ty)?;
    match op {
        CmpOp::Eq | CmpOp::Ne if !capabilities.equality => Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: type_id {} does not support equality comparison",
            ty.0
        ))),
        CmpOp::Lt | CmpOp::Lte | CmpOp::Gt | CmpOp::Gte if !capabilities.ordering => {
            Err(TabulaError::InvalidIr(format!(
                "instruction {instr_idx}: type_id {} does not support ordering comparison",
                ty.0
            )))
        }
        _ => Ok(()),
    }
}

fn expr_type(
    expr: &ValueExpr,
    param_types: &[TypeId],
    slot_types: &[Option<TypeId>],
) -> Option<TypeId> {
    match expr {
        ValueExpr::Literal(v) => Some(v.type_id()),
        ValueExpr::Param(idx) => param_types.get(*idx as usize).copied(),
        ValueExpr::Slot(idx) => slot_types.get(*idx as usize).copied().flatten(),
    }
}

fn type_capabilities(
    profile_catalog: &ProfileCatalog,
    type_id: TypeId,
) -> Result<TypeCapabilities, TabulaError> {
    profile_catalog
        .types
        .iter()
        .find(|descriptor| descriptor.type_id == type_id)
        .map(|descriptor| descriptor.capabilities)
        .ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown type_id {} in profile catalog", type_id.0))
        })
}

fn ensure_arithmetic_type(
    profile_catalog: &ProfileCatalog,
    type_id: TypeId,
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let capabilities = type_capabilities(profile_catalog, type_id)?;
    if !capabilities.arithmetic {
        return Err(TabulaError::InvalidIr(format!(
            "instruction {instr_idx}: type_id {} does not support arithmetic",
            type_id.0
        )));
    }
    Ok(())
}
