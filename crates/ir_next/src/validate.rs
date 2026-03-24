use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID};

use crate::model::*;

struct TableValidationInfo {
    key_tys: Vec<TypeRef>,
    fields: BTreeMap<FieldId, TypeRef>,
}

pub fn validate_program(program: &Program) -> Result<(), TabulaError> {
    let state = validate_state(&program.state)?;
    let context = unique_fields(
        &program.context.fields,
        |field| field.id,
        "duplicate context field ID",
    )?;
    let consts = unique_fields(
        &program.const_pool.entries,
        |entry| entry.id,
        "duplicate const ID",
    )?;
    for entry in &program.const_pool.entries {
        ensure_type(entry.value.type_id(), entry.ty, "const entry type mismatch")?;
    }
    let relations = unique_fields(
        &program.relation_manifest.entries,
        |entry| entry.id,
        "duplicate relation ID",
    )?;
    for entry in &program.relation_manifest.entries {
        validate_relation_entry(entry)?;
    }
    let capabilities = unique_fields(
        &program.capability_manifest.entries,
        |entry| entry.id,
        "duplicate capability ID",
    )?;
    let events = unique_fields(
        &program.event_manifest.entries,
        |entry| entry.id,
        "duplicate event ID",
    )?;
    let entry_ids = unique_fields(&program.entries, |entry| entry.id, "duplicate entry ID")?;
    let _ = entry_ids;
    for entry in &program.entries {
        validate_entry(
            entry,
            &state,
            &context,
            &consts,
            &relations,
            &capabilities,
            &events,
        )?;
    }
    Ok(())
}

fn validate_state(
    state: &StateSchema,
) -> Result<BTreeMap<TableId, TableValidationInfo>, TabulaError> {
    let mut tables = BTreeMap::new();
    let mut seen_tables = BTreeSet::new();
    for table in &state.tables {
        if !seen_tables.insert(table.id) {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate table ID {}",
                table.id.0
            )));
        }
        if table.key_tys.is_empty() {
            return Err(TabulaError::InvalidIr(format!(
                "table {} must declare at least one key type",
                table.id.0
            )));
        }
        let mut fields = BTreeMap::new();
        let mut seen_fields = BTreeSet::new();
        for field in &table.fields {
            if !seen_fields.insert(field.id) {
                return Err(TabulaError::InvalidIr(format!(
                    "duplicate field ID {} in table {}",
                    field.id.0, table.id.0
                )));
            }
            fields.insert(field.id, field.ty);
        }
        tables.insert(
            table.id,
            TableValidationInfo {
                key_tys: table.key_tys.clone(),
                fields,
            },
        );
    }
    Ok(tables)
}

fn unique_fields<'a, T, Id: Copy + Ord>(
    values: &'a [T],
    id: impl Fn(&T) -> Id,
    message: &str,
) -> Result<BTreeMap<Id, &'a T>, TabulaError> {
    let mut map = BTreeMap::new();
    for value in values {
        let key = id(value);
        if map.insert(key, value).is_some() {
            return Err(TabulaError::InvalidIr(message.into()));
        }
    }
    Ok(map)
}

fn validate_relation_entry(entry: &RelationManifestEntry) -> Result<(), TabulaError> {
    match &entry.binding {
        RelationBinding::EnumSet { values } => {
            if entry.descriptor.inputs.len() != 1 || !entry.descriptor.outputs.is_empty() {
                return Err(TabulaError::InvalidIr(format!(
                    "enum relation {} must have exactly one input and no outputs",
                    entry.descriptor.symbol
                )));
            }
            for value in values {
                ensure_type(
                    value.type_id(),
                    entry.descriptor.inputs[0],
                    "enum relation value type mismatch",
                )?;
            }
        }
        RelationBinding::Map { rows } => {
            for row in rows {
                if row.inputs.len() != entry.descriptor.inputs.len()
                    || row.outputs.len() != entry.descriptor.outputs.len()
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "map relation {} row arity mismatch",
                        entry.descriptor.symbol
                    )));
                }
                for (value, expected) in row.inputs.iter().zip(&entry.descriptor.inputs) {
                    ensure_type(value.type_id(), *expected, "relation input type mismatch")?;
                }
                for (value, expected) in row.outputs.iter().zip(&entry.descriptor.outputs) {
                    ensure_type(value.type_id(), *expected, "relation output type mismatch")?;
                }
            }
        }
    }
    Ok(())
}

fn validate_entry(
    entry: &Entry,
    state: &BTreeMap<TableId, TableValidationInfo>,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    relations: &BTreeMap<RelationId, &RelationManifestEntry>,
    capabilities: &BTreeMap<CapabilityId, &CapabilityDescriptor>,
    events: &BTreeMap<EventId, &EventDescriptor>,
) -> Result<(), TabulaError> {
    unique_fields(&entry.params, |param| param.id, "duplicate param ID")?;
    unique_fields(&entry.body.locals, |local| local.id, "duplicate local ID")?;

    if entry.kind == EntryKind::Tx && entry.return_policy != ReturnPolicy::Unit {
        return Err(TabulaError::InvalidIr(format!(
            "tx entry {} must use unit return policy",
            entry.symbol
        )));
    }
    if entry.kind == EntryKind::Tx && !entry.returns.is_empty() {
        return Err(TabulaError::InvalidIr(format!(
            "tx entry {} must not return values",
            entry.symbol
        )));
    }

    let params = entry
        .params
        .iter()
        .map(|param| (param.id, param.ty))
        .collect::<BTreeMap<_, _>>();
    let locals = entry
        .body
        .locals
        .iter()
        .map(|local| (local.id, local.ty))
        .collect::<BTreeMap<_, _>>();
    let mut assigned = BTreeSet::new();
    let mut seen_return = false;

    for (index, op) in entry.body.ops.iter().enumerate() {
        if matches!(op, Op::Return { .. }) {
            if seen_return {
                return Err(TabulaError::InvalidIr(format!(
                    "entry {} contains multiple Return ops",
                    entry.symbol
                )));
            }
            seen_return = true;
            if index + 1 != entry.body.ops.len() {
                return Err(TabulaError::InvalidIr(format!(
                    "entry {} has Return before end of body",
                    entry.symbol
                )));
            }
        } else if seen_return {
            return Err(TabulaError::InvalidIr(format!(
                "entry {} contains op after Return",
                entry.symbol
            )));
        }
        validate_op(
            op,
            entry,
            state,
            context,
            consts,
            relations,
            capabilities,
            events,
            &params,
            &locals,
            &mut assigned,
        )?;
    }

    if !seen_return {
        return Err(TabulaError::InvalidIr(format!(
            "entry {} is missing Return",
            entry.symbol
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_op(
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
    validate_guard(op, locals, assigned)?;
    match op {
        Op::Arith { dst, lhs, rhs, .. } => {
            let lhs_ty = value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = value_type(rhs, context, consts, params, locals, assigned)?;
            ensure_type(lhs_ty, rhs_ty, "arith operands must have same type")?;
            assign_local(*dst, lhs_ty, locals, assigned)?;
        }
        Op::Cmp { dst, lhs, rhs, .. } => {
            let lhs_ty = value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = value_type(rhs, context, consts, params, locals, assigned)?;
            ensure_type(lhs_ty, rhs_ty, "cmp operands must have same type")?;
            assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::Not { dst, src } => {
            ensure_type(
                value_type(src, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "not expects bool source",
            )?;
            assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::And { dst, lhs, rhs } | Op::Or { dst, lhs, rhs } => {
            ensure_type(
                value_type(lhs, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "boolean op lhs must be bool",
            )?;
            ensure_type(
                value_type(rhs, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "boolean op rhs must be bool",
            )?;
            assign_local(*dst, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => {
            ensure_type(
                value_type(cond, context, consts, params, locals, assigned)?,
                TYPE_BOOL_ID,
                "select condition must be bool",
            )?;
            let true_ty = value_type(if_true, context, consts, params, locals, assigned)?;
            let false_ty = value_type(if_false, context, consts, params, locals, assigned)?;
            ensure_type(true_ty, false_ty, "select branch type mismatch")?;
            assign_local(*dst, true_ty, locals, assigned)?;
        }
        Op::Hash { dst, inputs, .. } => {
            validate_value_tuple(inputs, context, consts, params, locals, assigned)?;
            assign_local(*dst, TYPE_BYTES32_ID, locals, assigned)?;
        }
        Op::DivMod {
            dst_q,
            dst_r,
            lhs,
            rhs,
            ..
        } => {
            let lhs_ty = value_type(lhs, context, consts, params, locals, assigned)?;
            let rhs_ty = value_type(rhs, context, consts, params, locals, assigned)?;
            ensure_type(lhs_ty, rhs_ty, "divmod operands must have same type")?;
            assign_local(*dst_q, lhs_ty, locals, assigned)?;
            assign_local(*dst_r, lhs_ty, locals, assigned)?;
        }
        Op::ReadState {
            dst_value,
            dst_present,
            table,
            key,
            field,
            ..
        } => {
            validate_key_tuple(
                key,
                resolve_key_tys(state, *table)?,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            let field_ty = resolve_field_type(state, *table, *field)?;
            assign_local(*dst_value, field_ty, locals, assigned)?;
            assign_local(*dst_present, TYPE_BOOL_ID, locals, assigned)?;
        }
        Op::WriteState {
            table,
            key,
            field,
            value,
            ..
        } => {
            validate_key_tuple(
                key,
                resolve_key_tys(state, *table)?,
                context,
                consts,
                params,
                locals,
                assigned,
            )?;
            ensure_type(
                value_type(value, context, consts, params, locals, assigned)?,
                resolve_field_type(state, *table, *field)?,
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
            let _ = resolve_field_type(state, *table, *field)?;
            validate_key_tuple(
                key,
                resolve_key_tys(state, *table)?,
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
            dsts,
            table,
            field,
            query,
            ..
        } => {
            let field_ty = resolve_field_type(state, *table, *field)?;
            let key_tys = resolve_key_tys(state, *table)?;
            validate_state_property_query(
                query, key_tys, context, consts, params, locals, assigned,
            )?;
            validate_property_dsts(query, dsts, field_ty, key_tys, locals)?;
            for dst in dsts {
                let local_ty = local_type(*dst, locals)?;
                assign_local(*dst, local_ty, locals, assigned)?;
            }
        }
        Op::Assert { cond, .. } => {
            ensure_type(
                value_type(cond, context, consts, params, locals, assigned)?,
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
                assign_local(*dst, *ty, locals, assigned)?;
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
                assign_local(*dst, *ty, locals, assigned)?;
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

fn validate_state_property_query(
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
            validate_key_tuple(key, key_tys, context, consts, params, locals, assigned)
        }
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            validate_key_tuple(lower, key_tys, context, consts, params, locals, assigned)?;
            validate_key_tuple(upper, key_tys, context, consts, params, locals, assigned)
        }
    }
}

fn validate_property_dsts(
    query: &StatePropertyQuery,
    dsts: &[LocalId],
    field_ty: TypeRef,
    key_tys: &[TypeRef],
    locals: &BTreeMap<LocalId, TypeRef>,
) -> Result<(), TabulaError> {
    match query {
        StatePropertyQuery::Minimum
        | StatePropertyQuery::Maximum
        | StatePropertyQuery::Successor { .. }
        | StatePropertyQuery::Predecessor { .. } => {
            if dsts.len() != 3 {
                return Err(TabulaError::InvalidIr(
                    "row-oriented property reads require exactly 3 destinations".into(),
                ));
            }
            ensure_type(
                local_type(dsts[0], locals)?,
                field_ty,
                "property value dst type mismatch",
            )?;
            if let [key_ty] = key_tys {
                ensure_type(
                    local_type(dsts[1], locals)?,
                    *key_ty,
                    "property key dst type mismatch",
                )?;
            }
            ensure_type(
                local_type(dsts[2], locals)?,
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

fn validate_relation_args(
    args: &ValueTupleRef,
    expected: &[TypeRef],
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    if args.0.len() != expected.len() {
        return Err(TabulaError::InvalidIr(format!(
            "tuple arity mismatch: expected {}, got {}",
            expected.len(),
            args.0.len()
        )));
    }
    for (value, expected_ty) in args.0.iter().zip(expected) {
        ensure_type(
            value_type(value, context, consts, params, locals, assigned)?,
            *expected_ty,
            "tuple element type mismatch",
        )?;
    }
    Ok(())
}

fn validate_value_tuple(
    values: &ValueTupleRef,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    for value in &values.0 {
        let _ = value_type(value, context, consts, params, locals, assigned)?;
    }
    Ok(())
}

fn validate_key_tuple(
    values: &ValueTupleRef,
    expected: &[TypeRef],
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    validate_relation_args(values, expected, context, consts, params, locals, assigned)
}

fn validate_guard(
    op: &Op,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &BTreeSet<LocalId>,
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
        | Op::EmitEvent { guard, .. } => *guard,
        Op::Arith { .. }
        | Op::Cmp { .. }
        | Op::Not { .. }
        | Op::And { .. }
        | Op::Or { .. }
        | Op::Select { .. }
        | Op::Hash { .. }
        | Op::Return { .. } => None,
    };
    if let Some(guard) = guard {
        let ty = local_type(guard.0, locals)?;
        ensure_type(ty, TYPE_BOOL_ID, "guard local must be bool")?;
        if !assigned.contains(&guard.0) {
            return Err(TabulaError::InvalidIr(format!(
                "guard local {} used before assignment",
                guard.0.0
            )));
        }
    } else {
        match op {
            Op::Arith { .. }
            | Op::Cmp { .. }
            | Op::Not { .. }
            | Op::And { .. }
            | Op::Or { .. }
            | Op::Select { .. }
            | Op::Hash { .. }
            | Op::Return { .. } => {}
            _ => {}
        }
    }
    Ok(())
}

fn value_type(
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
        ValueRef::Const(id) => consts
            .get(id)
            .map(|entry| entry.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown const ID {}", id.0))),
        ValueRef::Local(id) => {
            if !assigned.contains(id) {
                return Err(TabulaError::InvalidIr(format!(
                    "local {} used before assignment",
                    id.0
                )));
            }
            local_type(*id, locals)
        }
    }
}

fn local_type(id: LocalId, locals: &BTreeMap<LocalId, TypeRef>) -> Result<TypeRef, TabulaError> {
    locals
        .get(&id)
        .copied()
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown local ID {}", id.0)))
}

fn assign_local(
    id: LocalId,
    expected_ty: TypeRef,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &mut BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    let declared_ty = local_type(id, locals)?;
    ensure_type(declared_ty, expected_ty, "local declared type mismatch")?;
    if !assigned.insert(id) {
        return Err(TabulaError::InvalidIr(format!(
            "local {} assigned more than once",
            id.0
        )));
    }
    Ok(())
}

fn resolve_field_type(
    state: &BTreeMap<TableId, TableValidationInfo>,
    table: TableId,
    field: FieldId,
) -> Result<TypeRef, TabulaError> {
    let table_info = state
        .get(&table)
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table ID {}", table.0)))?;
    table_info
        .fields
        .get(&field)
        .copied()
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown field ID {}", field.0)))
}

fn resolve_key_tys<'a>(
    state: &'a BTreeMap<TableId, TableValidationInfo>,
    table: TableId,
) -> Result<&'a [TypeRef], TabulaError> {
    state
        .get(&table)
        .map(|table_info| table_info.key_tys.as_slice())
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table ID {}", table.0)))
}

fn ensure_type(actual: TypeRef, expected: TypeRef, msg: &str) -> Result<(), TabulaError> {
    if actual != expected {
        return Err(TabulaError::InvalidIr(format!(
            "{}: expected type {}, got {}",
            msg, expected.0, actual.0
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tabula_core::PortableValue;
    use tabula_profile::{TYPE_BOOL_ID, TYPE_U64_ID};

    use super::*;

    fn base_program(entry: Entry) -> Program {
        Program {
            program_id: ProgramId(0),
            state: StateSchema {
                tables: vec![TableSchema {
                    id: TableId(1),
                    symbol: "accounts".into(),
                    key_tys: vec![TYPE_U64_ID],
                    fields: vec![FieldSchema {
                        id: FieldId(0),
                        symbol: "balance".into(),
                        ty: TYPE_U64_ID,
                    }],
                }],
            },
            context: ContextSchema { fields: vec![] },
            const_pool: ConstantPool { entries: vec![] },
            relation_manifest: RelationManifest { entries: vec![] },
            capability_manifest: CapabilityManifest { entries: vec![] },
            event_manifest: EventManifest { entries: vec![] },
            entries: vec![entry],
        }
    }

    fn u64_literal(value: u64) -> ValueRef {
        ValueRef::Literal(PortableValue::new(
            TYPE_U64_ID,
            value.to_le_bytes().to_vec(),
        ))
    }

    #[test]
    fn accepts_minimal_query() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "balance".into(),
            kind: EntryKind::Query,
            params: vec![ParamDecl {
                id: ParamId(0),
                symbol: "owner".into(),
                ty: TYPE_U64_ID,
            }],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::ReadState {
                        guard: None,
                        dst_value: LocalId(0),
                        dst_present: LocalId(1),
                        table: TableId(1),
                        key: ValueTupleRef(vec![ValueRef::Param(ParamId(0))]),
                        field: FieldId(0),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        });
        validate_program(&program).unwrap();
    }

    #[test]
    fn rejects_query_write() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad".into(),
            kind: EntryKind::Query,
            params: vec![ParamDecl {
                id: ParamId(0),
                symbol: "owner".into(),
                ty: TYPE_U64_ID,
            }],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![],
                ops: vec![
                    Op::WriteState {
                        guard: None,
                        table: TableId(1),
                        key: ValueTupleRef(vec![ValueRef::Param(ParamId(0))]),
                        field: FieldId(0),
                        value: ValueRef::Literal(PortableValue::new(
                            TYPE_U64_ID,
                            1u64.to_le_bytes().to_vec(),
                        )),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                            TYPE_U64_ID,
                            0u64.to_le_bytes().to_vec(),
                        ))]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_duplicate_entry_ids() {
        let entry = Entry {
            id: EntryId(0),
            symbol: "balance".into(),
            kind: EntryKind::Query,
            params: vec![ParamDecl {
                id: ParamId(0),
                symbol: "owner".into(),
                ty: TYPE_U64_ID,
            }],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::ReadState {
                        guard: None,
                        dst_value: LocalId(0),
                        dst_present: LocalId(1),
                        table: TableId(1),
                        key: ValueTupleRef(vec![ValueRef::Param(ParamId(0))]),
                        field: FieldId(0),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        };
        let mut program = base_program(entry.clone());
        program.entries.push(entry);
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_unknown_local_reference() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_ref".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![],
                ops: vec![Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Local(LocalId(99))]),
                }],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_return_arity_mismatch() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_return".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![],
                ops: vec![Op::Return {
                    values: ValueTupleRef(vec![]),
                }],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_non_bool_guard_local() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_guard".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(2),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::Arith {
                        dst: LocalId(1),
                        op: ArithOp::Add,
                        lhs: u64_literal(1),
                        rhs: u64_literal(2),
                    },
                    Op::ReadState {
                        guard: Some(GuardRef(LocalId(1))),
                        dst_value: LocalId(0),
                        dst_present: LocalId(2),
                        table: TableId(1),
                        key: ValueTupleRef(vec![u64_literal(0)]),
                        field: FieldId(0),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_unknown_relation_reference() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_relation".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                }],
                ops: vec![
                    Op::AssertRelation {
                        guard: None,
                        relation: RelationId(99),
                        args: ValueTupleRef(vec![u64_literal(1)]),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![u64_literal(0)]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_query_tx_only_capability() {
        let mut program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_capability".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                }],
                ops: vec![
                    Op::CallCapability {
                        guard: None,
                        capability: CapabilityId(7),
                        inputs: ValueTupleRef(vec![u64_literal(1)]),
                        dsts: vec![LocalId(0)],
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        });
        program
            .capability_manifest
            .entries
            .push(CapabilityDescriptor {
                id: CapabilityId(7),
                symbol: "tx_only".into(),
                inputs: vec![TYPE_U64_ID],
                outputs: vec![TYPE_U64_ID],
                totality: CapabilityTotality::Total,
                query_policy: CapabilityQueryPolicy::TxOnly,
                proof_visibility: CapabilityProofVisibility::Journaled,
            });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_empty_key_schema() {
        let mut program = base_program(Entry {
            id: EntryId(0),
            symbol: "ok".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![],
                ops: vec![Op::Return {
                    values: ValueTupleRef(vec![u64_literal(0)]),
                }],
            },
        });
        program.state.tables[0].key_tys.clear();
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_state_key_arity_mismatch() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_key_arity".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::ReadState {
                        guard: None,
                        dst_value: LocalId(0),
                        dst_present: LocalId(1),
                        table: TableId(1),
                        key: ValueTupleRef(vec![u64_literal(1), u64_literal(2)]),
                        field: FieldId(0),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_state_key_type_mismatch() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_key_type".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::ReadState {
                        guard: None,
                        dst_value: LocalId(0),
                        dst_present: LocalId(1),
                        table: TableId(1),
                        key: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                            TYPE_BOOL_ID,
                            vec![1],
                        ))]),
                        field: FieldId(0),
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn rejects_property_query_embedded_key_type_mismatch() {
        let program = base_program(Entry {
            id: EntryId(0),
            symbol: "bad_property_key".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(2),
                        ty: TYPE_BOOL_ID,
                    },
                    LocalDecl {
                        id: LocalId(3),
                        ty: TYPE_U64_ID,
                    },
                ],
                ops: vec![
                    Op::ReadStateProperty {
                        guard: None,
                        dsts: vec![LocalId(0), LocalId(1), LocalId(2)],
                        table: TableId(1),
                        field: FieldId(0),
                        query: StatePropertyQuery::Successor {
                            key: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                                TYPE_BOOL_ID,
                                vec![1],
                            ))]),
                        },
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                            TYPE_U64_ID,
                            0u64.to_le_bytes().to_vec(),
                        ))]),
                    },
                ],
            },
        });
        assert!(validate_program(&program).is_err());
    }

    #[test]
    fn validated_program_rejects_invalid_raw_program() {
        let mut program = base_program(Entry {
            id: EntryId(0),
            symbol: "ok".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![],
                ops: vec![Op::Return {
                    values: ValueTupleRef(vec![u64_literal(0)]),
                }],
            },
        });
        program.state.tables[0].key_tys.clear();
        assert!(ValidatedProgram::try_from(program).is_err());
    }

    #[test]
    fn accepts_multi_component_key_schema_for_row_property_queries() {
        let mut program = base_program(Entry {
            id: EntryId(0),
            symbol: "multi_key_property".into(),
            kind: EntryKind::Query,
            params: vec![],
            returns: vec![TYPE_U64_ID, TYPE_U64_ID, TYPE_BOOL_ID],
            return_policy: ReturnPolicy::Explicit,
            body: Body {
                locals: vec![
                    LocalDecl {
                        id: LocalId(0),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(1),
                        ty: TYPE_U64_ID,
                    },
                    LocalDecl {
                        id: LocalId(2),
                        ty: TYPE_BOOL_ID,
                    },
                ],
                ops: vec![
                    Op::ReadStateProperty {
                        guard: None,
                        dsts: vec![LocalId(0), LocalId(1), LocalId(2)],
                        table: TableId(1),
                        field: FieldId(0),
                        query: StatePropertyQuery::Minimum,
                    },
                    Op::Return {
                        values: ValueTupleRef(vec![
                            ValueRef::Local(LocalId(0)),
                            ValueRef::Local(LocalId(1)),
                            ValueRef::Local(LocalId(2)),
                        ]),
                    },
                ],
            },
        });
        program.state.tables[0].key_tys = vec![TYPE_U64_ID, TYPE_U64_ID];
        assert!(validate_program(&program).is_ok());
    }
}
