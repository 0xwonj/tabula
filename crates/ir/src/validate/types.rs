use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;

use super::{
    ConstId, ConstantEntry, ContextField, ContextFieldId, FieldId, LocalId, ParamId, TableId,
    TableValidationInfo, TypeRef, ValueRef,
};

pub(super) fn value_type(
    value: &ValueRef,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<TypeRef, TabulaError> {
    match value {
        ValueRef::Literal(value) => Ok(value.type_id()),
        ValueRef::Param(id) => params
            .get(id)
            .copied()
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown param ID {}", id.0))),
        ValueRef::Context(id) => context
            .get(id)
            .map(|field| field.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown context field ID {}", id.0))),
        ValueRef::Local(id) => {
            let ty = local_type(*id, locals)?;
            if !assigned.contains(id) {
                return Err(TabulaError::InvalidIr(format!(
                    "local {} used before assignment",
                    id.0
                )));
            }
            Ok(ty)
        }
        ValueRef::Const(id) => consts
            .get(id)
            .map(|entry| entry.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown const ID {}", id.0))),
    }
}

pub(super) fn local_type(
    id: LocalId,
    locals: &BTreeMap<LocalId, TypeRef>,
) -> Result<TypeRef, TabulaError> {
    locals
        .get(&id)
        .copied()
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown local ID {}", id.0)))
}

pub(super) fn assign_local(
    id: LocalId,
    ty: TypeRef,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &mut BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    ensure_type(
        local_type(id, locals)?,
        ty,
        "local assignment type mismatch",
    )?;
    assigned.insert(id);
    Ok(())
}

pub(super) fn resolve_field_type(
    state: &BTreeMap<TableId, TableValidationInfo>,
    table: TableId,
    field: FieldId,
) -> Result<TypeRef, TabulaError> {
    state
        .get(&table)
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table ID {}", table.0)))?
        .fields
        .get(&field)
        .copied()
        .ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown field ID {} on table {}", field.0, table.0))
        })
}

pub(super) fn resolve_key_tys(
    state: &BTreeMap<TableId, TableValidationInfo>,
    table: TableId,
) -> Result<&[TypeRef], TabulaError> {
    Ok(state
        .get(&table)
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table ID {}", table.0)))?
        .key_tys
        .as_slice())
}

pub(super) fn ensure_type(
    actual: TypeRef,
    expected: TypeRef,
    msg: &str,
) -> Result<(), TabulaError> {
    if actual == expected {
        Ok(())
    } else {
        Err(TabulaError::InvalidIr(format!(
            "{msg}: expected {}, got {}",
            expected.0, actual.0
        )))
    }
}
