use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID};

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;

use super::{
    CapabilityDescriptor, CapabilityId, CapabilityQueryPolicy, ConstId, ConstantEntry,
    ContextField, ContextFieldId, Entry, EntryKind, EventDescriptor, EventId, LocalId, Op, ParamId,
    RelationId, RelationManifestEntry, StatePropertyQuery, TableId, TableValidationInfo, TypeRef,
    ValueTupleRef, types, values,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_op(
    op: &Op,
    entry: &Entry,
    state: &BTreeMap<TableId, TableValidationInfo>,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    relations: &BTreeMap<RelationId, &RelationManifestEntry>,
    capabilities: &BTreeMap<CapabilityId, &CapabilityDescriptor>,
    events: &BTreeMap<EventId, &EventDescriptor>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &mut BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    let guard = match op {
        Op::DivMod { guard, .. }
        | Op::ReadState { guard, .. }
        | Op::WriteState { guard, .. }
        | Op::DeleteState { guard, .. }
        | Op::ReadStateProperty { guard, .. }
        | Op::Assert { guard, .. }
        | Op::AssertRelation { guard, .. }
        | Op::EvalRelation { guard, .. }
        | Op::CallCapability { guard, .. }
        | Op::EmitEvent { guard, .. } => guard.as_ref(),
        Op::Arith { .. }
        | Op::Cmp { .. }
        | Op::Not { .. }
        | Op::And { .. }
        | Op::Or { .. }
        | Op::Select { .. }
        | Op::Hash { .. }
        | Op::Return { .. } => None,
    };
    values::validate_guard(guard, locals, assigned)?;

    match op {
        Op::Arith { dst, lhs, rhs, .. } => {
            let lhs_ty = types::value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = types::value_type(rhs, context, consts, params, locals, assigned)?;
            types::ensure_type(lhs_ty, rhs_ty, "arith operands must have same type")?;
            types::assign_local(*dst, lhs_ty, locals, assigned)?;
        }
        Op::Cmp { dst, lhs, rhs, .. } => {
            let lhs_ty = types::value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = types::value_type(rhs, context, consts, params, locals, assigned)?;
            types::ensure_type(lhs_ty, rhs_ty, "cmp operands must have same type")?;
            types::assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::Not { dst, src } => {
            types::ensure_type(
                types::value_type(src, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "not expects bool source",
            )?;
            types::assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::And { dst, lhs, rhs } | Op::Or { dst, lhs, rhs } => {
            types::ensure_type(
                types::value_type(lhs, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "boolean op lhs must be bool",
            )?;
            types::ensure_type(
                types::value_type(rhs, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "boolean op rhs must be bool",
            )?;
            types::assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            types::ensure_type(
                types::value_type(cond, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "select condition must be bool",
            )?;
            let true_ty = types::value_type(if_true, context, consts, params, locals, assigned)?;
            let false_ty = types::value_type(if_false, context, consts, params, locals, assigned)?;
            types::ensure_type(true_ty, false_ty, "select branch type mismatch")?;
            types::assign_local(*dst, true_ty, locals, assigned)?;
        }
        Op::Hash { dst, inputs, .. } => {
            let _ =
                values::validate_value_tuple(inputs, context, consts, params, locals, assigned)?;
            types::assign_local(*dst, TYPE_BYTES32_ID, locals, assigned)?;
        }
        Op::DivMod {
            dst_q,
            dst_r,
            lhs,
            rhs,
            ..
        } => {
            let lhs_ty = types::value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = types::value_type(rhs, context, consts, params, locals, assigned)?;
            types::ensure_type(lhs_ty, rhs_ty, "divmod operands must have same type")?;
            types::assign_local(*dst_q, lhs_ty, locals, assigned)?;
            types::assign_local(*dst_r, lhs_ty, locals, assigned)?;
        }
        Op::ReadState {
            dst_value,
            dst_present,
            table,
            key,
            field,
            ..
        } => {
            values::validate_key_tuple(
                key,
                types::resolve_key_tys(state, *table)?,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            let field_ty = types::resolve_field_type(state, *table, *field)?;
            types::assign_local(*dst_value, field_ty, locals, assigned)?;
            types::assign_local(*dst_present, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::WriteState {
            table,
            key,
            field,
            value,
            ..
        } => {
            values::validate_key_tuple(
                key,
                types::resolve_key_tys(state, *table)?,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            types::ensure_type(
                types::value_type(value, context, consts, params, locals, assigned)?,
                types::resolve_field_type(state, *table, *field)?,
                "state write value type mismatch",
            )?;
            if entry.kind == EntryKind::Query {
                return Err(TabulaError::InvalidIr(format!(
                    "query entry {} may not write state",
                    entry.symbol
                )));
            }
        }
        Op::DeleteState {
            table, key, field, ..
        } => {
            let _ = types::resolve_field_type(state, *table, *field)?;
            values::validate_key_tuple(
                key,
                types::resolve_key_tys(state, *table)?,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            if entry.kind == EntryKind::Query {
                return Err(TabulaError::InvalidIr(format!(
                    "query entry {} may not delete state",
                    entry.symbol
                )));
            }
        }
        Op::ReadStateProperty {
            dst_value,
            dst_key_components,
            dst_is_null,
            table,
            field,
            query,
            ..
        } => {
            let field_ty = types::resolve_field_type(state, *table, *field)?;
            let key_tys = types::resolve_key_tys(state, *table)?;
            validate_state_property_query(
                query, key_tys, context, consts, params, locals, assigned,
            )?;
            validate_property_dsts(
                query,
                *dst_value,
                dst_key_components,
                *dst_is_null,
                field_ty,
                key_tys,
                locals,
            )?;
            types::assign_local(*dst_value, field_ty, locals, assigned)?;
            for (dst, key_ty) in dst_key_components.iter().zip(key_tys.iter()) {
                types::assign_local(*dst, *key_ty, locals, assigned)?;
            }
            types::assign_local(*dst_is_null, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::Assert { cond, .. } => {
            types::ensure_type(
                types::value_type(cond, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "assert condition must be bool",
            )?;
        }
        Op::AssertRelation { relation, args, .. } => {
            let relation = relations.get(relation).ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown relation ID {}", relation.0))
            })?;
            if !relation.descriptor.outputs.is_empty() {
                return Err(TabulaError::InvalidIr(format!(
                    "assert relation {} requires output-free relation",
                    relation.descriptor.symbol
                )));
            }
            validate_relation_args(
                args,
                &relation.descriptor.inputs,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
        }
        Op::EvalRelation {
            relation,
            inputs,
            dsts,
            ..
        } => {
            let relation = relations.get(relation).ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown relation ID {}", relation.0))
            })?;
            validate_relation_args(
                inputs,
                &relation.descriptor.inputs,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            if dsts.len() != relation.descriptor.outputs.len() {
                return Err(TabulaError::InvalidIr(format!(
                    "eval relation {} destination arity mismatch",
                    relation.descriptor.symbol
                )));
            }
            for (dst, ty) in dsts.iter().zip(&relation.descriptor.outputs) {
                types::assign_local(*dst, *ty, locals, assigned)?;
            }
        }
        Op::CallCapability {
            capability,
            inputs,
            dsts,
            ..
        } => {
            let capability = capabilities.get(capability).ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown capability ID {}", capability.0))
            })?;
            validate_relation_args(
                inputs,
                &capability.inputs,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            if dsts.len() != capability.outputs.len() {
                return Err(TabulaError::InvalidIr(format!(
                    "capability {} destination arity mismatch",
                    capability.symbol
                )));
            }
            if entry.kind == EntryKind::Query
                && capability.query_policy == CapabilityQueryPolicy::TxOnly
            {
                return Err(TabulaError::InvalidIr(format!(
                    "query entry {} may not call tx-only capability {}",
                    entry.symbol, capability.symbol
                )));
            }
            for (dst, ty) in dsts.iter().zip(&capability.outputs) {
                types::assign_local(*dst, *ty, locals, assigned)?;
            }
        }
        Op::EmitEvent { event, args, .. } => {
            let event = events
                .get(event)
                .ok_or_else(|| TabulaError::InvalidIr(format!("unknown event ID {}", event.0)))?;
            validate_relation_args(
                args,
                &event.fields,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            if entry.kind == EntryKind::Query {
                return Err(TabulaError::InvalidIr(format!(
                    "query entry {} may not emit events",
                    entry.symbol
                )));
            }
        }
        Op::Return { values } => {
            validate_relation_args(
                values,
                &entry.returns,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
        }
    }

    Ok(())
}

pub(super) fn validate_state_property_query(
    query: &StatePropertyQuery,
    key_tys: &[TypeRef],
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    match query {
        StatePropertyQuery::Minimum
        | StatePropertyQuery::Maximum
        | StatePropertyQuery::Aggregate { .. } => Ok(()),
        StatePropertyQuery::Successor { key } | StatePropertyQuery::Predecessor { key } => {
            values::validate_key_tuple(key, key_tys, context, consts, params, locals, assigned)
        }
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            values::validate_key_tuple(lower, key_tys, context, consts, params, locals, assigned)?;
            values::validate_key_tuple(upper, key_tys, context, consts, params, locals, assigned)
        }
    }
}

pub(super) fn validate_property_dsts(
    query: &StatePropertyQuery,
    dst_value: LocalId,
    dst_key_components: &[LocalId],
    dst_is_null: LocalId,
    field_ty: TypeRef,
    key_tys: &[TypeRef],
    locals: &BTreeMap<LocalId, TypeRef>,
) -> Result<(), TabulaError> {
    match query {
        StatePropertyQuery::Minimum
        | StatePropertyQuery::Maximum
        | StatePropertyQuery::Successor { .. }
        | StatePropertyQuery::Predecessor { .. } => {
            if dst_key_components.len() != key_tys.len() {
                return Err(TabulaError::InvalidIr(
                    "property reads must declare one key destination per key component".into(),
                ));
            }
            types::ensure_type(
                types::local_type(dst_value, locals)?,
                field_ty,
                "property value dst type mismatch",
            )?;
            for (dst, key_ty) in dst_key_components.iter().zip(key_tys.iter()) {
                types::ensure_type(
                    types::local_type(*dst, locals)?,
                    *key_ty,
                    "property key dst type mismatch",
                )?;
            }
            types::ensure_type(
                types::local_type(dst_is_null, locals)?,
                TYPE_BOOL_ID,
                "property null-flag dst type mismatch",
            )?;
            Ok(())
        }
        StatePropertyQuery::Aggregate { .. } | StatePropertyQuery::NonExistenceRange { .. } => {
            Ok(())
        }
    }
}

pub(super) fn validate_relation_args(
    values: &ValueTupleRef,
    expected: &[TypeRef],
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    if values.0.len() != expected.len() {
        return Err(TabulaError::InvalidIr(format!(
            "tuple arity mismatch: expected {}, got {}",
            expected.len(),
            values.0.len()
        )));
    }
    for (value, expected_ty) in values.0.iter().zip(expected) {
        types::ensure_type(
            types::value_type(value, context, consts, params, locals, assigned)?,
            *expected_ty,
            "tuple element type mismatch",
        )?;
    }
    Ok(())
}
