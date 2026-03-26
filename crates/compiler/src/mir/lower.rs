use std::collections::BTreeMap;

use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_profile::TYPE_BOOL_ID;

use super::analysis::AnalyzedProgram;
use super::model::{
    Callable, CallableKind, MatchArm, MatchPattern, Op, Program, Terminator, ValueOp,
};

pub fn lower_to_canonical(program: &AnalyzedProgram) -> Result<ir::Program, TabulaError> {
    LowerCx::new(program.program()).lower_program()
}

struct LowerCx<'a> {
    program: &'a Program,
}

impl<'a> LowerCx<'a> {
    fn new(program: &'a Program) -> Self {
        Self { program }
    }

    fn lower_program(&self) -> Result<ir::Program, TabulaError> {
        let mut entries = Vec::new();
        for callable in &self.program.callables {
            if matches!(callable.kind, CallableKind::Query | CallableKind::Tx) {
                entries.push(self.lower_entry(callable)?);
            } else {
                return Err(TabulaError::InvalidIr(format!(
                    "function {} must be eliminated before canonical IR lowering",
                    callable.symbol
                )));
            }
        }
        Ok(ir::Program {
            program_id: self.program.program_id,
            state: self.program.state.clone(),
            context: self.program.context.clone(),
            const_pool: self.program.const_pool.clone(),
            relation_manifest: self.program.relation_manifest.clone(),
            capability_manifest: self.program.capability_manifest.clone(),
            event_manifest: self.program.event_manifest.clone(),
            entries,
        })
    }

    fn lower_entry(&self, callable: &Callable) -> Result<ir::Entry, TabulaError> {
        let mut builder = LowerBuilder::new(callable);
        let mut env = LowerEnv::for_callable(callable);
        let returns =
            self.lower_root_region(&callable.body.region, &mut builder, &mut env, None)?;
        builder.ops.push(ir::Op::Return {
            values: ir::ValueTupleRef(returns),
        });
        Ok(ir::Entry {
            id: ir::EntryId(callable.id.0),
            symbol: callable.symbol.clone(),
            kind: match callable.kind {
                CallableKind::Query => ir::EntryKind::Query,
                CallableKind::Tx => ir::EntryKind::Tx,
                CallableKind::Function => unreachable!(),
            },
            params: callable.params.clone(),
            returns: callable.returns.clone(),
            return_policy: match callable.kind {
                CallableKind::Query => ir::ReturnPolicy::Explicit,
                CallableKind::Tx => ir::ReturnPolicy::Unit,
                CallableKind::Function => unreachable!(),
            },
            body: ir::Body {
                locals: builder.locals,
                ops: builder.ops,
            },
        })
    }

    fn lower_root_region(
        &self,
        region: &super::Region,
        builder: &mut LowerBuilder,
        env: &mut LowerEnv,
        current_guard: Option<ir::LocalId>,
    ) -> Result<Vec<ir::ValueRef>, TabulaError> {
        self.lower_ops(region, builder, env, current_guard)?;
        match &region.terminator {
            Terminator::Return { values } => env.resolve_tuple(values),
            Terminator::Yield { .. } => Err(TabulaError::InvalidIr(
                "root MIR region must terminate with Return".into(),
            )),
        }
    }

    fn lower_nested_region(
        &self,
        region: &super::Region,
        builder: &mut LowerBuilder,
        env: &mut LowerEnv,
        current_guard: Option<ir::LocalId>,
    ) -> Result<Vec<ir::ValueRef>, TabulaError> {
        self.lower_ops(region, builder, env, current_guard)?;
        match &region.terminator {
            Terminator::Yield { values } => env.resolve_tuple(values),
            Terminator::Return { .. } => Err(TabulaError::InvalidIr(
                "nested MIR region must terminate with Yield".into(),
            )),
        }
    }

    fn lower_ops(
        &self,
        region: &super::Region,
        builder: &mut LowerBuilder,
        env: &mut LowerEnv,
        current_guard: Option<ir::LocalId>,
    ) -> Result<(), TabulaError> {
        for op in &region.ops {
            match op {
                Op::BindValue { dst, value } => {
                    let canonical_dst = builder.alloc_local(env.local_type(*dst)?);
                    match value {
                        ValueOp::Arith { op, lhs, rhs } => builder.ops.push(ir::Op::Arith {
                            dst: canonical_dst,
                            op: *op,
                            lhs: env.resolve_value(lhs)?,
                            rhs: env.resolve_value(rhs)?,
                        }),
                        ValueOp::Cmp { op, lhs, rhs } => builder.ops.push(ir::Op::Cmp {
                            dst: canonical_dst,
                            op: *op,
                            lhs: env.resolve_value(lhs)?,
                            rhs: env.resolve_value(rhs)?,
                        }),
                        ValueOp::Not { src } => builder.ops.push(ir::Op::Not {
                            dst: canonical_dst,
                            src: env.resolve_value(src)?,
                        }),
                        ValueOp::And { lhs, rhs } => builder.ops.push(ir::Op::And {
                            dst: canonical_dst,
                            lhs: env.resolve_value(lhs)?,
                            rhs: env.resolve_value(rhs)?,
                        }),
                        ValueOp::Or { lhs, rhs } => builder.ops.push(ir::Op::Or {
                            dst: canonical_dst,
                            lhs: env.resolve_value(lhs)?,
                            rhs: env.resolve_value(rhs)?,
                        }),
                        ValueOp::Select {
                            cond,
                            if_true,
                            if_false,
                        } => builder.ops.push(ir::Op::Select {
                            dst: canonical_dst,
                            cond: env.resolve_value(cond)?,
                            if_true: env.resolve_value(if_true)?,
                            if_false: env.resolve_value(if_false)?,
                        }),
                        ValueOp::Hash { family, inputs } => builder.ops.push(ir::Op::Hash {
                            dst: canonical_dst,
                            family: *family,
                            inputs: env.resolve_tuple_ref(inputs)?,
                        }),
                    }
                    env.bind_local(*dst, ir::ValueRef::Local(canonical_dst))?;
                }
                Op::DivMod {
                    dst_q,
                    dst_r,
                    lhs,
                    rhs,
                } => {
                    let q = builder.alloc_local(env.local_type(*dst_q)?);
                    let r = builder.alloc_local(env.local_type(*dst_r)?);
                    builder.ops.push(ir::Op::DivMod {
                        guard: current_guard.map(ir::GuardRef),
                        dst_q: q,
                        dst_r: r,
                        lhs: env.resolve_value(lhs)?,
                        rhs: env.resolve_value(rhs)?,
                    });
                    env.bind_local(*dst_q, ir::ValueRef::Local(q))?;
                    env.bind_local(*dst_r, ir::ValueRef::Local(r))?;
                }
                Op::ReadState {
                    dst_value,
                    dst_present,
                    table,
                    key,
                    field,
                } => {
                    let value = builder.alloc_local(env.local_type(*dst_value)?);
                    let present = builder.alloc_local(env.local_type(*dst_present)?);
                    builder.ops.push(ir::Op::ReadState {
                        guard: current_guard.map(ir::GuardRef),
                        dst_value: value,
                        dst_present: present,
                        table: *table,
                        key: env.resolve_tuple_ref(key)?,
                        field: *field,
                    });
                    env.bind_local(*dst_value, ir::ValueRef::Local(value))?;
                    env.bind_local(*dst_present, ir::ValueRef::Local(present))?;
                }
                Op::WriteState {
                    table,
                    key,
                    field,
                    value,
                } => builder.ops.push(ir::Op::WriteState {
                    guard: current_guard.map(ir::GuardRef),
                    table: *table,
                    key: env.resolve_tuple_ref(key)?,
                    field: *field,
                    value: env.resolve_value(value)?,
                }),
                Op::DeleteState { table, key, field } => builder.ops.push(ir::Op::DeleteState {
                    guard: current_guard.map(ir::GuardRef),
                    table: *table,
                    key: env.resolve_tuple_ref(key)?,
                    field: *field,
                }),
                Op::ReadStateProperty {
                    dsts,
                    table,
                    field,
                    query,
                } => {
                    let mut canonical_dsts = Vec::new();
                    for dst in dsts {
                        let local = builder.alloc_local(env.local_type(*dst)?);
                        canonical_dsts.push(local);
                        env.bind_local(*dst, ir::ValueRef::Local(local))?;
                    }
                    builder.ops.push(ir::Op::ReadStateProperty {
                        guard: current_guard.map(ir::GuardRef),
                        dsts: canonical_dsts,
                        table: *table,
                        field: *field,
                        query: Self::resolve_property_query(query, env)?,
                    });
                }
                Op::Assert { cond } => builder.ops.push(ir::Op::Assert {
                    guard: current_guard.map(ir::GuardRef),
                    cond: env.resolve_value(cond)?,
                }),
                Op::AssertRelation { relation, args } => builder.ops.push(ir::Op::AssertRelation {
                    guard: current_guard.map(ir::GuardRef),
                    relation: *relation,
                    args: env.resolve_tuple_ref(args)?,
                }),
                Op::EvalRelation {
                    relation,
                    inputs,
                    dsts,
                } => {
                    let mut canonical_dsts = Vec::new();
                    for dst in dsts {
                        let local = builder.alloc_local(env.local_type(*dst)?);
                        canonical_dsts.push(local);
                        env.bind_local(*dst, ir::ValueRef::Local(local))?;
                    }
                    builder.ops.push(ir::Op::EvalRelation {
                        guard: current_guard.map(ir::GuardRef),
                        relation: *relation,
                        inputs: env.resolve_tuple_ref(inputs)?,
                        dsts: canonical_dsts,
                    });
                }
                Op::CallCapability {
                    capability,
                    inputs,
                    dsts,
                } => {
                    let mut canonical_dsts = Vec::new();
                    for dst in dsts {
                        let local = builder.alloc_local(env.local_type(*dst)?);
                        canonical_dsts.push(local);
                        env.bind_local(*dst, ir::ValueRef::Local(local))?;
                    }
                    builder.ops.push(ir::Op::CallCapability {
                        guard: current_guard.map(ir::GuardRef),
                        capability: *capability,
                        inputs: env.resolve_tuple_ref(inputs)?,
                        dsts: canonical_dsts,
                    });
                }
                Op::CallFunction { .. } => {
                    return Err(TabulaError::InvalidIr(
                        "CallFunction must be eliminated before canonical lowering".into(),
                    ));
                }
                Op::EmitEvent { event, args } => builder.ops.push(ir::Op::EmitEvent {
                    guard: current_guard.map(ir::GuardRef),
                    event: *event,
                    args: env.resolve_tuple_ref(args)?,
                }),
                Op::If {
                    dsts,
                    cond,
                    then_region,
                    else_region,
                } => self.lower_if(
                    dsts,
                    cond,
                    then_region,
                    else_region,
                    builder,
                    env,
                    current_guard,
                )?,
                Op::Match {
                    dsts,
                    scrutinee,
                    arms,
                    default,
                } => self.lower_match(
                    dsts,
                    scrutinee,
                    arms,
                    default.as_ref(),
                    builder,
                    env,
                    current_guard,
                )?,
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_if(
        &self,
        dsts: &[ir::LocalId],
        cond: &ir::ValueRef,
        then_region: &super::Region,
        else_region: &super::Region,
        builder: &mut LowerBuilder,
        env: &mut LowerEnv,
        current_guard: Option<ir::LocalId>,
    ) -> Result<(), TabulaError> {
        let cond_local = ensure_bool_local(builder, env.resolve_value(cond)?)?;
        let then_guard = combine_guard(builder, current_guard, cond_local)?;
        let not_cond = emit_not(builder, cond_local);
        let else_guard = combine_guard(builder, current_guard, not_cond)?;

        let mut then_env = env.clone();
        let then_values =
            self.lower_nested_region(then_region, builder, &mut then_env, Some(then_guard))?;
        let mut else_env = env.clone();
        let else_values =
            self.lower_nested_region(else_region, builder, &mut else_env, Some(else_guard))?;

        if then_values.len() != dsts.len() || else_values.len() != dsts.len() {
            return Err(TabulaError::InvalidIr(
                "if region yield arity did not match destination arity during lowering".into(),
            ));
        }
        for ((dst, then_value), else_value) in dsts.iter().zip(then_values).zip(else_values) {
            let local = builder.alloc_local(env.local_type(*dst)?);
            builder.ops.push(ir::Op::Select {
                dst: local,
                cond: ir::ValueRef::Local(cond_local),
                if_true: then_value,
                if_false: else_value,
            });
            env.bind_local(*dst, ir::ValueRef::Local(local))?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_match(
        &self,
        dsts: &[ir::LocalId],
        scrutinee: &ir::ValueRef,
        arms: &[MatchArm],
        default: Option<&super::Region>,
        builder: &mut LowerBuilder,
        env: &mut LowerEnv,
        current_guard: Option<ir::LocalId>,
    ) -> Result<(), TabulaError> {
        let scrutinee = env.resolve_value(scrutinee)?;
        let mut active_arms = Vec::<(ir::LocalId, Vec<ir::ValueRef>)>::new();
        let mut catch_all_values = None;
        let mut prior_taken: Option<ir::LocalId> = None;

        for arm in arms {
            match &arm.pattern {
                MatchPattern::Literal(value) => {
                    let cmp = builder.alloc_local(TYPE_BOOL_ID);
                    builder.ops.push(ir::Op::Cmp {
                        dst: cmp,
                        op: ir::CmpOp::Eq,
                        lhs: scrutinee.clone(),
                        rhs: ir::ValueRef::Literal(value.clone()),
                    });
                    let selector = match prior_taken {
                        Some(prior) => {
                            let not_prior = emit_not(builder, prior);
                            let candidate = emit_and(
                                builder,
                                ir::ValueRef::Local(not_prior),
                                ir::ValueRef::Local(cmp),
                            );
                            combine_guard(builder, current_guard, candidate)?
                        }
                        None => combine_guard(builder, current_guard, cmp)?,
                    };
                    let mut arm_env = env.clone();
                    let values = self.lower_nested_region(
                        &arm.region,
                        builder,
                        &mut arm_env,
                        Some(selector),
                    )?;
                    active_arms.push((selector, values));
                    prior_taken = Some(match prior_taken {
                        Some(prior) => emit_or(
                            builder,
                            ir::ValueRef::Local(prior),
                            ir::ValueRef::Local(selector),
                        ),
                        None => selector,
                    });
                }
                MatchPattern::Wildcard => {
                    let guard = Self::compute_catch_all_guard(builder, current_guard, prior_taken);
                    let mut arm_env = env.clone();
                    let values =
                        self.lower_nested_region(&arm.region, builder, &mut arm_env, Some(guard))?;
                    catch_all_values = Some(values);
                }
            }
        }

        if let Some(default_region) = default {
            let guard = Self::compute_catch_all_guard(builder, current_guard, prior_taken);
            let mut default_env = env.clone();
            catch_all_values = Some(self.lower_nested_region(
                default_region,
                builder,
                &mut default_env,
                Some(guard),
            )?);
        }

        if dsts.is_empty() {
            return Ok(());
        }
        let mut merged = catch_all_values.ok_or_else(|| {
            TabulaError::InvalidIr("value-producing match lowering requires default values".into())
        })?;
        if merged.len() != dsts.len() {
            return Err(TabulaError::InvalidIr(
                "match catch-all yield arity mismatch during lowering".into(),
            ));
        }
        for (selector, values) in active_arms.into_iter().rev() {
            if values.len() != dsts.len() {
                return Err(TabulaError::InvalidIr(
                    "match arm yield arity mismatch during lowering".into(),
                ));
            }
            for (index, value) in values.into_iter().enumerate() {
                let merged_local = builder.alloc_local(env.local_type(dsts[index])?);
                builder.ops.push(ir::Op::Select {
                    dst: merged_local,
                    cond: ir::ValueRef::Local(selector),
                    if_true: value,
                    if_false: merged[index].clone(),
                });
                merged[index] = ir::ValueRef::Local(merged_local);
            }
        }
        for (dst, value) in dsts.iter().zip(merged) {
            env.bind_local(*dst, value)?;
        }
        Ok(())
    }

    fn resolve_property_query(
        query: &ir::StatePropertyQuery,
        env: &LowerEnv,
    ) -> Result<ir::StatePropertyQuery, TabulaError> {
        Ok(match query {
            ir::StatePropertyQuery::Minimum => ir::StatePropertyQuery::Minimum,
            ir::StatePropertyQuery::Maximum => ir::StatePropertyQuery::Maximum,
            ir::StatePropertyQuery::Aggregate { kind } => {
                ir::StatePropertyQuery::Aggregate { kind: *kind }
            }
            ir::StatePropertyQuery::Successor { key } => ir::StatePropertyQuery::Successor {
                key: env.resolve_tuple_ref(key)?,
            },
            ir::StatePropertyQuery::Predecessor { key } => ir::StatePropertyQuery::Predecessor {
                key: env.resolve_tuple_ref(key)?,
            },
            ir::StatePropertyQuery::NonExistenceRange { lower, upper } => {
                ir::StatePropertyQuery::NonExistenceRange {
                    lower: env.resolve_tuple_ref(lower)?,
                    upper: env.resolve_tuple_ref(upper)?,
                }
            }
        })
    }

    fn compute_catch_all_guard(
        builder: &mut LowerBuilder,
        current_guard: Option<ir::LocalId>,
        prior_taken: Option<ir::LocalId>,
    ) -> ir::LocalId {
        match (current_guard, prior_taken) {
            (Some(current_guard), Some(prior_taken)) => {
                let not_prior = emit_not(builder, prior_taken);
                emit_and(
                    builder,
                    ir::ValueRef::Local(current_guard),
                    ir::ValueRef::Local(not_prior),
                )
            }
            (Some(current_guard), None) => current_guard,
            (None, Some(prior_taken)) => emit_not(builder, prior_taken),
            (None, None) => materialize_true(builder),
        }
    }
}

fn ensure_bool_local(
    builder: &mut LowerBuilder,
    value: ir::ValueRef,
) -> Result<ir::LocalId, TabulaError> {
    match value {
        ir::ValueRef::Local(id) => Ok(id),
        other => {
            let local = builder.alloc_local(TYPE_BOOL_ID);
            builder.ops.push(ir::Op::Or {
                dst: local,
                lhs: other,
                rhs: bool_literal(false),
            });
            Ok(local)
        }
    }
}

fn combine_guard(
    builder: &mut LowerBuilder,
    current_guard: Option<ir::LocalId>,
    guard: ir::LocalId,
) -> Result<ir::LocalId, TabulaError> {
    if let Some(current_guard) = current_guard {
        Ok(emit_and(
            builder,
            ir::ValueRef::Local(current_guard),
            ir::ValueRef::Local(guard),
        ))
    } else {
        Ok(guard)
    }
}

fn emit_not(builder: &mut LowerBuilder, src: ir::LocalId) -> ir::LocalId {
    let local = builder.alloc_local(TYPE_BOOL_ID);
    builder.ops.push(ir::Op::Not {
        dst: local,
        src: ir::ValueRef::Local(src),
    });
    local
}

fn emit_and(builder: &mut LowerBuilder, lhs: ir::ValueRef, rhs: ir::ValueRef) -> ir::LocalId {
    let local = builder.alloc_local(TYPE_BOOL_ID);
    builder.ops.push(ir::Op::And {
        dst: local,
        lhs,
        rhs,
    });
    local
}

fn emit_or(builder: &mut LowerBuilder, lhs: ir::ValueRef, rhs: ir::ValueRef) -> ir::LocalId {
    let local = builder.alloc_local(TYPE_BOOL_ID);
    builder.ops.push(ir::Op::Or {
        dst: local,
        lhs,
        rhs,
    });
    local
}

fn materialize_true(builder: &mut LowerBuilder) -> ir::LocalId {
    let local = builder.alloc_local(TYPE_BOOL_ID);
    builder.ops.push(ir::Op::Or {
        dst: local,
        lhs: bool_literal(true),
        rhs: bool_literal(false),
    });
    local
}

fn bool_literal(value: bool) -> ir::ValueRef {
    ir::ValueRef::Literal(PortableValue::new(TYPE_BOOL_ID, vec![u8::from(value)]))
}

struct LowerBuilder {
    locals: Vec<ir::LocalDecl>,
    ops: Vec<ir::Op>,
    next_local_id: u32,
}

impl LowerBuilder {
    fn new(callable: &Callable) -> Self {
        let next_local_id = callable
            .body
            .locals
            .iter()
            .map(|local| local.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        Self {
            locals: Vec::new(),
            ops: Vec::new(),
            next_local_id,
        }
    }

    fn alloc_local(&mut self, ty: ir::TypeRef) -> ir::LocalId {
        let id = ir::LocalId(self.next_local_id);
        self.next_local_id = self.next_local_id.saturating_add(1);
        self.locals.push(ir::LocalDecl { id, ty });
        id
    }
}

#[derive(Clone)]
struct LowerEnv {
    param_bindings: BTreeMap<ir::ParamId, ir::ValueRef>,
    local_bindings: BTreeMap<ir::LocalId, ir::ValueRef>,
    local_types: BTreeMap<ir::LocalId, ir::TypeRef>,
}

impl LowerEnv {
    fn for_callable(callable: &Callable) -> Self {
        let param_bindings = callable
            .params
            .iter()
            .map(|param| (param.id, ir::ValueRef::Param(param.id)))
            .collect();
        let local_types = callable
            .body
            .locals
            .iter()
            .map(|local| (local.id, local.ty))
            .collect();
        Self {
            param_bindings,
            local_bindings: BTreeMap::new(),
            local_types,
        }
    }

    fn bind_local(&mut self, local: ir::LocalId, value: ir::ValueRef) -> Result<(), TabulaError> {
        if self.local_bindings.insert(local, value).is_some() {
            return Err(TabulaError::InvalidIr(format!(
                "MIR local {} lowered more than once",
                local.0
            )));
        }
        Ok(())
    }

    fn resolve_value(&self, value: &ir::ValueRef) -> Result<ir::ValueRef, TabulaError> {
        Ok(match value {
            ir::ValueRef::Literal(value) => ir::ValueRef::Literal(value.clone()),
            ir::ValueRef::Param(id) => self
                .param_bindings
                .get(id)
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr(format!("unknown MIR param {}", id.0)))?,
            ir::ValueRef::Context(id) => ir::ValueRef::Context(*id),
            ir::ValueRef::Local(id) => self
                .local_bindings
                .get(id)
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr(format!("unknown MIR local {}", id.0)))?,
            ir::ValueRef::Const(id) => ir::ValueRef::Const(*id),
        })
    }

    fn resolve_tuple(&self, tuple: &ir::ValueTupleRef) -> Result<Vec<ir::ValueRef>, TabulaError> {
        tuple
            .0
            .iter()
            .map(|value| self.resolve_value(value))
            .collect()
    }

    fn resolve_tuple_ref(
        &self,
        tuple: &ir::ValueTupleRef,
    ) -> Result<ir::ValueTupleRef, TabulaError> {
        Ok(ir::ValueTupleRef(self.resolve_tuple(tuple)?))
    }

    fn local_type(&self, local: ir::LocalId) -> Result<ir::TypeRef, TabulaError> {
        self.local_types
            .get(&local)
            .copied()
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown MIR local {}", local.0)))
    }
}
