#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID};

use super::model::*;

#[derive(Debug, Clone)]
pub struct VerifiedProgram(Program);

impl VerifiedProgram {
    pub fn program(&self) -> &Program {
        &self.0
    }

    pub fn into_program(self) -> Program {
        self.0
    }
}

pub fn verify_program(program: Program) -> Result<VerifiedProgram, TabulaError> {
    VerifyCx::new(&program)?.verify()?;
    Ok(VerifiedProgram(program))
}

pub fn validate_program(program: &Program) -> Result<(), TabulaError> {
    verify_program(program.clone()).map(|_| ())
}

struct VerifyCx<'a> {
    program: &'a Program,
    callables: BTreeMap<CallableId, &'a Callable>,
}

impl<'a> VerifyCx<'a> {
    fn new(program: &'a Program) -> Result<Self, TabulaError> {
        let callables = unique_fields(
            &program.callables,
            |callable| callable.id,
            "duplicate callable ID",
        )?;
        Ok(Self { program, callables })
    }

    fn verify(&self) -> Result<(), TabulaError> {
        self.verify_top_level_semantic_universe()?;
        for callable in &self.program.callables {
            self.verify_callable(callable)?;
        }
        Ok(())
    }

    fn verify_top_level_semantic_universe(&self) -> Result<(), TabulaError> {
        ir::validate_program(&ir::Program {
            program_id: self.program.program_id,
            state: self.program.state.clone(),
            context: self.program.context.clone(),
            const_pool: self.program.const_pool.clone(),
            relation_manifest: self.program.relation_manifest.clone(),
            capability_manifest: self.program.capability_manifest.clone(),
            event_manifest: self.program.event_manifest.clone(),
            entries: Vec::new(),
        })
    }

    fn verify_callable(&self, callable: &Callable) -> Result<(), TabulaError> {
        unique_fields(&callable.params, |param| param.id, "duplicate param ID")?;
        unique_fields(
            &callable.body.locals,
            |local| local.id,
            "duplicate local ID",
        )?;
        if callable.kind == CallableKind::Tx && !callable.returns.is_empty() {
            return Err(TabulaError::InvalidIr(format!(
                "tx callable {} must not declare return values",
                callable.symbol
            )));
        }
        let params = callable
            .params
            .iter()
            .map(|param| (param.id, param.ty))
            .collect::<BTreeMap<_, _>>();
        let locals = callable
            .body
            .locals
            .iter()
            .map(|local| (local.id, local.ty))
            .collect::<BTreeMap<_, _>>();
        let available = BTreeSet::new();
        let mut assigned = BTreeSet::new();
        self.verify_region(
            &callable.body.region,
            RegionKind::Root(callable),
            &params,
            &locals,
            &available,
            &mut assigned,
        )
    }

    fn verify_region(
        &self,
        region: &Region,
        kind: RegionKind<'_>,
        params: &BTreeMap<ParamId, TypeRef>,
        locals: &BTreeMap<LocalId, TypeRef>,
        available_in: &BTreeSet<LocalId>,
        assigned: &mut BTreeSet<LocalId>,
    ) -> Result<(), TabulaError> {
        let mut available = available_in.clone();
        for op in &region.ops {
            self.verify_op(op, params, locals, &available, assigned)?;
            for dst in op.defines_locals() {
                available.insert(dst);
            }
        }
        match kind {
            RegionKind::Root(callable) => match &region.terminator {
                Terminator::Return { values } => validate_tuple(
                    values,
                    &callable.returns,
                    params,
                    locals,
                    &available,
                    self.program,
                )?,
                Terminator::Yield { .. } => {
                    return Err(TabulaError::InvalidIr(format!(
                        "root region of callable {} must terminate with Return",
                        callable.symbol
                    )));
                }
            },
            RegionKind::Nested { expected_yields } => match &region.terminator {
                Terminator::Yield { values } => {
                    if values.0.len() != expected_yields.len() {
                        return Err(TabulaError::InvalidIr(
                            "region yield arity does not match control-op destinations".into(),
                        ));
                    }
                    for (value, dst) in values.0.iter().zip(expected_yields) {
                        ensure_type(
                            value_type(value, params, locals, &available, self.program)?,
                            local_type(*dst, locals)?,
                            "region yield type mismatch",
                        )?;
                    }
                }
                Terminator::Return { .. } => {
                    return Err(TabulaError::InvalidIr(
                        "nested MIR regions must terminate with Yield".into(),
                    ));
                }
            },
        }
        Ok(())
    }

    fn verify_op(
        &self,
        op: &Op,
        params: &BTreeMap<ParamId, TypeRef>,
        locals: &BTreeMap<LocalId, TypeRef>,
        available: &BTreeSet<LocalId>,
        assigned: &mut BTreeSet<LocalId>,
    ) -> Result<(), TabulaError> {
        match op {
            Op::BindValue { dst, value } => {
                let ty = validate_value_op(value, params, locals, available, self.program)?;
                assign_local(*dst, ty, locals, assigned)?;
            }
            Op::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                let lhs_ty = value_type(lhs, params, locals, available, self.program)?;
                ensure_type(
                    lhs_ty,
                    value_type(rhs, params, locals, available, self.program)?,
                    "divmod operand type mismatch",
                )?;
                assign_local(*dst_q, lhs_ty, locals, assigned)?;
                assign_local(*dst_r, lhs_ty, locals, assigned)?;
            }
            Op::ReadState {
                dst_value,
                dst_present,
                table,
                key,
                field,
            } => {
                let key_tys = table_key_tys(*table, self.program)?;
                validate_tuple(key, key_tys, params, locals, available, self.program)?;
                let field_ty = field_type(*table, *field, self.program)?;
                assign_local(*dst_value, field_ty, locals, assigned)?;
                assign_local(*dst_present, TYPE_BOOL_ID, locals, assigned)?;
            }
            Op::WriteState {
                table,
                key,
                field,
                value,
            } => {
                let key_tys = table_key_tys(*table, self.program)?;
                validate_tuple(key, key_tys, params, locals, available, self.program)?;
                ensure_type(
                    value_type(value, params, locals, available, self.program)?,
                    field_type(*table, *field, self.program)?,
                    "state write value type mismatch",
                )?;
            }
            Op::DeleteState { table, key, field } => {
                let _ = field_type(*table, *field, self.program)?;
                let key_tys = table_key_tys(*table, self.program)?;
                validate_tuple(key, key_tys, params, locals, available, self.program)?;
            }
            Op::ReadStateProperty {
                dsts,
                table,
                field,
                query,
            } => {
                let field_ty = field_type(*table, *field, self.program)?;
                let key_tys = table_key_tys(*table, self.program)?;
                validate_state_property_query(
                    query,
                    key_tys,
                    params,
                    locals,
                    available,
                    self.program,
                )?;
                validate_property_dsts(query, dsts, field_ty, key_tys, locals)?;
                for dst in dsts {
                    assign_local(*dst, local_type(*dst, locals)?, locals, assigned)?;
                }
            }
            Op::Assert { cond } => {
                ensure_type(
                    value_type(cond, params, locals, available, self.program)?,
                    TYPE_BOOL_ID,
                    "assert condition must be bool",
                )?;
            }
            Op::AssertRelation { relation, args } => {
                let relation = self
                    .program
                    .relation_manifest
                    .entries
                    .iter()
                    .find(|entry| entry.id == *relation)
                    .ok_or_else(|| {
                        TabulaError::InvalidIr(format!("unknown relation ID {}", relation.0))
                    })?;
                if !relation.descriptor.outputs.is_empty() {
                    return Err(TabulaError::InvalidIr(format!(
                        "assert relation {} requires output-free relation",
                        relation.descriptor.symbol
                    )));
                }
                validate_tuple(
                    args,
                    &relation.descriptor.inputs,
                    params,
                    locals,
                    available,
                    self.program,
                )?;
            }
            Op::EvalRelation {
                relation,
                inputs,
                dsts,
            } => {
                let relation = self
                    .program
                    .relation_manifest
                    .entries
                    .iter()
                    .find(|entry| entry.id == *relation)
                    .ok_or_else(|| {
                        TabulaError::InvalidIr(format!("unknown relation ID {}", relation.0))
                    })?;
                validate_tuple(
                    inputs,
                    &relation.descriptor.inputs,
                    params,
                    locals,
                    available,
                    self.program,
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
            } => {
                let capability = self
                    .program
                    .capability_manifest
                    .entries
                    .iter()
                    .find(|entry| entry.id == *capability)
                    .ok_or_else(|| {
                        TabulaError::InvalidIr(format!("unknown capability ID {}", capability.0))
                    })?;
                validate_tuple(
                    inputs,
                    &capability.inputs,
                    params,
                    locals,
                    available,
                    self.program,
                )?;
                if dsts.len() != capability.outputs.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "capability {} destination arity mismatch",
                        capability.symbol
                    )));
                }
                for (dst, ty) in dsts.iter().zip(&capability.outputs) {
                    assign_local(*dst, *ty, locals, assigned)?;
                }
            }
            Op::CallFunction {
                callee,
                inputs,
                dsts,
            } => {
                let callee = self.callables.get(callee).ok_or_else(|| {
                    TabulaError::InvalidIr(format!("unknown MIR callable {}", callee.0))
                })?;
                if callee.kind != CallableKind::Function {
                    return Err(TabulaError::InvalidIr(format!(
                        "CallFunction may only target Function callable, got {:?}",
                        callee.kind
                    )));
                }
                let callee_inputs = callable_input_types(callee);
                validate_tuple(
                    inputs,
                    &callee_inputs,
                    params,
                    locals,
                    available,
                    self.program,
                )?;
                if dsts.len() != callee.returns.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "call to {} destination arity mismatch",
                        callee.symbol
                    )));
                }
                for (dst, ty) in dsts.iter().zip(&callee.returns) {
                    assign_local(*dst, *ty, locals, assigned)?;
                }
            }
            Op::EmitEvent { event, args } => {
                let event = self
                    .program
                    .event_manifest
                    .entries
                    .iter()
                    .find(|entry| entry.id == *event)
                    .ok_or_else(|| {
                        TabulaError::InvalidIr(format!("unknown event ID {}", event.0))
                    })?;
                validate_tuple(args, &event.fields, params, locals, available, self.program)?;
            }
            Op::If {
                dsts,
                cond,
                then_region,
                else_region,
            } => {
                ensure_type(
                    value_type(cond, params, locals, available, self.program)?,
                    TYPE_BOOL_ID,
                    "if condition must be bool",
                )?;
                self.verify_region(
                    then_region,
                    RegionKind::Nested {
                        expected_yields: dsts,
                    },
                    params,
                    locals,
                    available,
                    assigned,
                )?;
                self.verify_region(
                    else_region,
                    RegionKind::Nested {
                        expected_yields: dsts,
                    },
                    params,
                    locals,
                    available,
                    assigned,
                )?;
                for dst in dsts {
                    assign_local(*dst, local_type(*dst, locals)?, locals, assigned)?;
                }
            }
            Op::Match {
                dsts,
                scrutinee,
                arms,
                default,
            } => {
                let scrutinee_ty = value_type(scrutinee, params, locals, available, self.program)?;
                let mut seen_wildcard = false;
                let mut seen_literals = Vec::new();
                for (index, arm) in arms.iter().enumerate() {
                    match &arm.pattern {
                        MatchPattern::Literal(value) => {
                            ensure_type(
                                value.type_id(),
                                scrutinee_ty,
                                "match literal pattern type mismatch",
                            )?;
                            if seen_literals.iter().any(|seen| seen == value) {
                                return Err(TabulaError::InvalidIr(
                                    "duplicate match literal pattern".into(),
                                ));
                            }
                            seen_literals.push(value.clone());
                        }
                        MatchPattern::Wildcard => {
                            if seen_wildcard {
                                return Err(TabulaError::InvalidIr(
                                    "match may contain at most one wildcard arm".into(),
                                ));
                            }
                            if index + 1 != arms.len() {
                                return Err(TabulaError::InvalidIr(
                                    "wildcard match arm must be last".into(),
                                ));
                            }
                            seen_wildcard = true;
                            if default.is_some() {
                                return Err(TabulaError::InvalidIr(
                                    "match may not use both wildcard arm and default region".into(),
                                ));
                            }
                        }
                    }
                    self.verify_region(
                        &arm.region,
                        RegionKind::Nested {
                            expected_yields: dsts,
                        },
                        params,
                        locals,
                        available,
                        assigned,
                    )?;
                }
                if !dsts.is_empty() && !seen_wildcard && default.is_none() {
                    return Err(TabulaError::InvalidIr(
                        "value-producing match requires wildcard arm or default region".into(),
                    ));
                }
                if let Some(default) = default {
                    self.verify_region(
                        default,
                        RegionKind::Nested {
                            expected_yields: dsts,
                        },
                        params,
                        locals,
                        available,
                        assigned,
                    )?;
                }
                for dst in dsts {
                    assign_local(*dst, local_type(*dst, locals)?, locals, assigned)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum RegionKind<'a> {
    Root(&'a Callable),
    Nested { expected_yields: &'a [LocalId] },
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

fn validate_tuple(
    values: &ValueTupleRef,
    expected: &[TypeRef],
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    available: &BTreeSet<LocalId>,
    program: &Program,
) -> Result<(), TabulaError> {
    if values.0.len() != expected.len() {
        return Err(TabulaError::InvalidIr(format!(
            "tuple arity mismatch: expected {}, got {}",
            expected.len(),
            values.0.len()
        )));
    }
    for (value, expected_ty) in values.0.iter().zip(expected) {
        ensure_type(
            value_type(value, params, locals, available, program)?,
            *expected_ty,
            "tuple element type mismatch",
        )?;
    }
    Ok(())
}

fn validate_state_property_query(
    query: &StatePropertyQuery,
    key_tys: &[TypeRef],
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    available: &BTreeSet<LocalId>,
    program: &Program,
) -> Result<(), TabulaError> {
    match query {
        StatePropertyQuery::Minimum | StatePropertyQuery::Maximum => Ok(()),
        StatePropertyQuery::Successor { key } | StatePropertyQuery::Predecessor { key } => {
            validate_tuple(key, key_tys, params, locals, available, program)
        }
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            validate_tuple(lower, key_tys, params, locals, available, program)?;
            validate_tuple(upper, key_tys, params, locals, available, program)
        }
        StatePropertyQuery::Aggregate { .. } => Ok(()),
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
            if key_tys.len() != 1 || dsts.len() != 3 {
                return Err(TabulaError::InvalidIr(
                    "row-oriented property reads require exactly three destinations".into(),
                ));
            }
            ensure_type(
                local_type(dsts[0], locals)?,
                field_ty,
                "property value dst type mismatch",
            )?;
            ensure_type(
                local_type(dsts[1], locals)?,
                key_tys[0],
                "property key dst type mismatch",
            )?;
            ensure_type(
                local_type(dsts[2], locals)?,
                TYPE_BOOL_ID,
                "property null-flag dst type mismatch",
            )
        }
        StatePropertyQuery::Aggregate { .. } => {
            if dsts.len() != 1 {
                return Err(TabulaError::InvalidIr(
                    "aggregate property reads require exactly one destination".into(),
                ));
            }
            ensure_type(
                local_type(dsts[0], locals)?,
                field_ty,
                "aggregate dst type mismatch",
            )
        }
        StatePropertyQuery::NonExistenceRange { .. } => Err(TabulaError::InvalidIr(
            "non-existence range property reads are not supported in MIR V1".into(),
        )),
    }
}

fn validate_value_op(
    value: &ValueOp,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    available: &BTreeSet<LocalId>,
    program: &Program,
) -> Result<TypeRef, TabulaError> {
    match value {
        ValueOp::Arith { lhs, rhs, .. } => {
            let lhs_ty = value_type(lhs, params, locals, available, program)?;
            ensure_type(
                lhs_ty,
                value_type(rhs, params, locals, available, program)?,
                "arith operand type mismatch",
            )?;
            Ok(lhs_ty)
        }
        ValueOp::Cmp { lhs, rhs, .. } => {
            let lhs_ty = value_type(lhs, params, locals, available, program)?;
            ensure_type(
                lhs_ty,
                value_type(rhs, params, locals, available, program)?,
                "cmp operand type mismatch",
            )?;
            Ok(TYPE_BOOL_ID)
        }
        ValueOp::Not { src } => {
            ensure_type(
                value_type(src, params, locals, available, program)?,
                TYPE_BOOL_ID,
                "not expects bool source",
            )?;
            Ok(TYPE_BOOL_ID)
        }
        ValueOp::And { lhs, rhs } | ValueOp::Or { lhs, rhs } => {
            ensure_type(
                value_type(lhs, params, locals, available, program)?,
                TYPE_BOOL_ID,
                "boolean operand must be bool",
            )?;
            ensure_type(
                value_type(rhs, params, locals, available, program)?,
                TYPE_BOOL_ID,
                "boolean operand must be bool",
            )?;
            Ok(TYPE_BOOL_ID)
        }
        ValueOp::Select {
            cond,
            if_true,
            if_false,
        } => {
            ensure_type(
                value_type(cond, params, locals, available, program)?,
                TYPE_BOOL_ID,
                "select condition must be bool",
            )?;
            let true_ty = value_type(if_true, params, locals, available, program)?;
            ensure_type(
                true_ty,
                value_type(if_false, params, locals, available, program)?,
                "select branch type mismatch",
            )?;
            Ok(true_ty)
        }
        ValueOp::Hash { inputs, .. } => {
            for value in &inputs.0 {
                let _ = value_type(value, params, locals, available, program)?;
            }
            Ok(TYPE_BYTES32_ID)
        }
    }
}

fn value_type(
    value: &ValueRef,
    params: &BTreeMap<ParamId, TypeRef>,
    locals: &BTreeMap<LocalId, TypeRef>,
    available: &BTreeSet<LocalId>,
    program: &Program,
) -> Result<TypeRef, TabulaError> {
    match value {
        ValueRef::Literal(value) => Ok(value.type_id()),
        ValueRef::Param(id) => params
            .get(id)
            .copied()
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown MIR param {}", id.0))),
        ValueRef::Context(id) => program
            .context
            .fields
            .iter()
            .find(|field| field.id == *id)
            .map(|field| field.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown context field {}", id.0))),
        ValueRef::Local(id) => {
            if !available.contains(id) {
                return Err(TabulaError::InvalidIr(format!(
                    "MIR local {} used before definition",
                    id.0
                )));
            }
            local_type(*id, locals)
        }
        ValueRef::Const(id) => program
            .const_pool
            .entries
            .iter()
            .find(|entry| entry.id == *id)
            .map(|entry| entry.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown const ID {}", id.0))),
    }
}

fn table_key_tys(table: TableId, program: &Program) -> Result<&[TypeRef], TabulaError> {
    let table = program
        .state
        .tables
        .iter()
        .find(|candidate| candidate.id == table)
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table ID {}", table.0)))?;
    Ok(&table.key_tys)
}

fn field_type(table: TableId, field: FieldId, program: &Program) -> Result<TypeRef, TabulaError> {
    program
        .state
        .tables
        .iter()
        .find(|candidate| candidate.id == table)
        .and_then(|table| table.fields.iter().find(|candidate| candidate.id == field))
        .map(|field| field.ty)
        .ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown field {} in table {}", field.0, table.0))
        })
}

fn local_type(local: LocalId, locals: &BTreeMap<LocalId, TypeRef>) -> Result<TypeRef, TabulaError> {
    locals
        .get(&local)
        .copied()
        .ok_or_else(|| TabulaError::InvalidIr(format!("unknown MIR local {}", local.0)))
}

fn callable_input_types(callable: &Callable) -> Vec<TypeRef> {
    callable.params.iter().map(|param| param.ty).collect()
}

fn ensure_type(actual: TypeRef, expected: TypeRef, message: &str) -> Result<(), TabulaError> {
    if actual != expected {
        return Err(TabulaError::InvalidIr(format!(
            "{message}: expected type {expected}, got {actual}",
        )));
    }
    Ok(())
}

fn assign_local(
    local: LocalId,
    expected_ty: TypeRef,
    locals: &BTreeMap<LocalId, TypeRef>,
    assigned: &mut BTreeSet<LocalId>,
) -> Result<(), TabulaError> {
    ensure_type(
        local_type(local, locals)?,
        expected_ty,
        "destination local type mismatch",
    )?;
    if !assigned.insert(local) {
        return Err(TabulaError::InvalidIr(format!(
            "MIR local {} assigned more than once",
            local.0
        )));
    }
    Ok(())
}
