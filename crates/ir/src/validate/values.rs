use tabula_profile::TYPE_BOOL_ID;

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;

use super::{
    ConstId, ConstantEntry, ContextField, ContextFieldId, GuardRef, LocalId, ParamId, TypeRef,
    ValueTupleRef, ops, types,
};

pub(super) fn validate_value_tuple(
    values: &ValueTupleRef,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<Vec<TypeRef>, TabulaError> {
    values
        .0
        .iter()
        .map(|value| types::value_type(value, context, consts, params, locals, assigned))
        .collect()
}

pub(super) fn validate_key_tuple(
    values: &ValueTupleRef,
    expected: &[TypeRef],
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    ops::validate_relation_args(values, expected, context, consts, params, locals, assigned)
}

pub(super) fn validate_guard(
    guard: Option<&GuardRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    if let Some(guard) = guard {
        let ty = types::local_type(guard.0, locals)?;
        types::ensure_type(ty, TYPE_BOOL_ID, "guard local must be bool")?;
        if !assigned.contains(&guard.0) {
            return Err(TabulaError::InvalidIr(format!(
                "guard local {} used before assignment",
                guard.0.0
            )));
        }
    }
    Ok(())
}
