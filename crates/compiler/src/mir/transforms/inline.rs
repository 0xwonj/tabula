#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_profile::TYPE_BOOL_ID;

use crate::mir::analysis::AnalyzedProgram;
use crate::mir::model::*;
use crate::mir::validate::{VerifiedProgram, verify_program};

pub fn inline_functions(program: &AnalyzedProgram) -> Result<VerifiedProgram, TabulaError> {
    let raw = program.program();
    let callables = raw
        .callables
        .iter()
        .map(|callable| (callable.id, callable))
        .collect::<BTreeMap<_, _>>();

    let mut normalized_callables = Vec::new();
    for callable in &raw.callables {
        if callable.kind == CallableKind::Function {
            continue;
        }
        let mut builder = InlineBuilder::new(callable);
        let mut env = InlineEnv::for_root(callable);
        let mut cx = InlineCx {
            callables: &callables,
            builder: &mut builder,
            call_stack: BTreeSet::new(),
        };
        let body = Body {
            locals: Vec::new(),
            region: cx.normalize_region(&callable.body.region, &mut env)?,
        };
        normalized_callables.push(Callable {
            id: callable.id,
            symbol: callable.symbol.clone(),
            kind: callable.kind,
            params: callable.params.clone(),
            returns: callable.returns.clone(),
            body: Body {
                locals: builder.finish_locals(),
                region: body.region,
            },
        });
    }

    verify_program(Program {
        program_id: raw.program_id,
        state: raw.state.clone(),
        context: raw.context.clone(),
        const_pool: raw.const_pool.clone(),
        relation_manifest: raw.relation_manifest.clone(),
        capability_manifest: raw.capability_manifest.clone(),
        event_manifest: raw.event_manifest.clone(),
        callables: normalized_callables,
    })
}

struct InlineCx<'a, 'b> {
    callables: &'a BTreeMap<CallableId, &'a Callable>,
    builder: &'b mut InlineBuilder,
    call_stack: BTreeSet<CallableId>,
}

impl<'a, 'b> InlineCx<'a, 'b> {
    fn normalize_region(
        &mut self,
        region: &Region,
        env: &mut InlineEnv,
    ) -> Result<Region, TabulaError> {
        let mut ops = Vec::new();
        for op in &region.ops {
            self.normalize_op(op, env, &mut ops)?;
        }
        Ok(Region {
            ops,
            terminator: Self::normalize_terminator(&region.terminator, env)?,
        })
    }

    fn normalize_terminator(
        terminator: &Terminator,
        env: &InlineEnv,
    ) -> Result<Terminator, TabulaError> {
        Ok(match terminator {
            Terminator::Yield { values } => Terminator::Yield {
                values: env.resolve_tuple_ref(values)?,
            },
            Terminator::Return { values } => Terminator::Return {
                values: env.resolve_tuple_ref(values)?,
            },
        })
    }

    fn normalize_op(
        &mut self,
        op: &Op,
        env: &mut InlineEnv,
        out: &mut Vec<Op>,
    ) -> Result<(), TabulaError> {
        match op {
            Op::BindValue { dst, value } => out.push(Op::BindValue {
                dst: env.define_local(*dst),
                value: Self::normalize_value_op(value, env)?,
            }),
            Op::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => out.push(Op::DivMod {
                dst_q: env.define_local(*dst_q),
                dst_r: env.define_local(*dst_r),
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            }),
            Op::ReadState {
                dst_value,
                dst_present,
                table,
                key,
                field,
            } => out.push(Op::ReadState {
                dst_value: env.define_local(*dst_value),
                dst_present: env.define_local(*dst_present),
                table: *table,
                key: env.resolve_tuple_ref(key)?,
                field: *field,
            }),
            Op::WriteState {
                table,
                key,
                field,
                value,
            } => out.push(Op::WriteState {
                table: *table,
                key: env.resolve_tuple_ref(key)?,
                field: *field,
                value: env.resolve_value(value)?,
            }),
            Op::DeleteState { table, key, field } => out.push(Op::DeleteState {
                table: *table,
                key: env.resolve_tuple_ref(key)?,
                field: *field,
            }),
            Op::ReadStateProperty {
                dsts,
                table,
                field,
                query,
            } => out.push(Op::ReadStateProperty {
                dsts: dsts.iter().map(|dst| env.define_local(*dst)).collect(),
                table: *table,
                field: *field,
                query: Self::normalize_property_query(query, env)?,
            }),
            Op::Assert { cond } => out.push(Op::Assert {
                cond: env.resolve_value(cond)?,
            }),
            Op::AssertRelation { relation, args } => out.push(Op::AssertRelation {
                relation: *relation,
                args: env.resolve_tuple_ref(args)?,
            }),
            Op::EvalRelation {
                relation,
                inputs,
                dsts,
            } => out.push(Op::EvalRelation {
                relation: *relation,
                inputs: env.resolve_tuple_ref(inputs)?,
                dsts: dsts.iter().map(|dst| env.define_local(*dst)).collect(),
            }),
            Op::CallCapability {
                capability,
                inputs,
                dsts,
            } => out.push(Op::CallCapability {
                capability: *capability,
                inputs: env.resolve_tuple_ref(inputs)?,
                dsts: dsts.iter().map(|dst| env.define_local(*dst)).collect(),
            }),
            Op::CallFunction {
                callee,
                inputs,
                dsts,
            } => self.inline_call(*callee, inputs, dsts, env, out)?,
            Op::EmitEvent { event, args } => out.push(Op::EmitEvent {
                event: *event,
                args: env.resolve_tuple_ref(args)?,
            }),
            Op::If {
                dsts,
                cond,
                then_region,
                else_region,
            } => {
                let mut then_env = env.clone();
                let mut else_env = env.clone();
                out.push(Op::If {
                    dsts: dsts.clone(),
                    cond: env.resolve_value(cond)?,
                    then_region: self.normalize_region(then_region, &mut then_env)?,
                    else_region: self.normalize_region(else_region, &mut else_env)?,
                });
            }
            Op::Match {
                dsts,
                scrutinee,
                arms,
                default,
            } => {
                let mut normalized_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let mut arm_env = env.clone();
                    normalized_arms.push(MatchArm {
                        pattern: arm.pattern.clone(),
                        region: self.normalize_region(&arm.region, &mut arm_env)?,
                    });
                }
                let normalized_default = if let Some(default) = default {
                    let mut default_env = env.clone();
                    Some(self.normalize_region(default, &mut default_env)?)
                } else {
                    None
                };
                out.push(Op::Match {
                    dsts: dsts.clone(),
                    scrutinee: env.resolve_value(scrutinee)?,
                    arms: normalized_arms,
                    default: normalized_default,
                });
            }
        }
        Ok(())
    }

    fn inline_call(
        &mut self,
        callee_id: CallableId,
        inputs: &ValueTupleRef,
        dsts: &[LocalId],
        env: &mut InlineEnv,
        out: &mut Vec<Op>,
    ) -> Result<(), TabulaError> {
        let callee = self.callables.get(&callee_id).ok_or_else(|| {
            TabulaError::InvalidIr(format!("unknown MIR callable {}", callee_id.0))
        })?;
        if callee.kind != CallableKind::Function {
            return Err(TabulaError::InvalidIr(format!(
                "CallFunction may only target Function callable, got {:?}",
                callee.kind
            )));
        }
        if !self.call_stack.insert(callee.id) {
            return Err(TabulaError::InvalidIr(format!(
                "recursive MIR function cycle detected while normalizing callable {}",
                callee.symbol
            )));
        }

        let resolved_inputs = env.resolve_tuple_ref(inputs)?;
        let mut callee_env = InlineEnv::for_inline_call(callee, &resolved_inputs, self.builder);
        let normalized_region = self.normalize_region(&callee.body.region, &mut callee_env)?;
        self.call_stack.remove(&callee.id);

        out.extend(normalized_region.ops);
        match normalized_region.terminator {
            Terminator::Return { values } | Terminator::Yield { values } => {
                if values.0.len() != dsts.len() {
                    return Err(TabulaError::InvalidIr(format!(
                        "call destination arity mismatch for callable {}",
                        callee.symbol
                    )));
                }
                for (dst, value) in dsts.iter().zip(values.0.iter()) {
                    out.push(Op::BindValue {
                        dst: env.define_local(*dst),
                        value: ValueOp::Select {
                            cond: ir::ValueRef::Literal(PortableValue::new(TYPE_BOOL_ID, vec![1])),
                            if_true: value.clone(),
                            if_false: value.clone(),
                        },
                    });
                }
            }
        }
        Ok(())
    }

    fn normalize_value_op(value: &ValueOp, env: &InlineEnv) -> Result<ValueOp, TabulaError> {
        Ok(match value {
            ValueOp::Arith { op, lhs, rhs } => ValueOp::Arith {
                op: *op,
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            },
            ValueOp::Cmp { op, lhs, rhs } => ValueOp::Cmp {
                op: *op,
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            },
            ValueOp::Not { src } => ValueOp::Not {
                src: env.resolve_value(src)?,
            },
            ValueOp::And { lhs, rhs } => ValueOp::And {
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            },
            ValueOp::Or { lhs, rhs } => ValueOp::Or {
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            },
            ValueOp::Select {
                cond,
                if_true,
                if_false,
            } => ValueOp::Select {
                cond: env.resolve_value(cond)?,
                if_true: env.resolve_value(if_true)?,
                if_false: env.resolve_value(if_false)?,
            },
            ValueOp::Hash { family, inputs } => ValueOp::Hash {
                family: *family,
                inputs: env.resolve_tuple_ref(inputs)?,
            },
        })
    }

    fn normalize_property_query(
        query: &StatePropertyQuery,
        env: &InlineEnv,
    ) -> Result<StatePropertyQuery, TabulaError> {
        Ok(match query {
            StatePropertyQuery::Minimum => StatePropertyQuery::Minimum,
            StatePropertyQuery::Maximum => StatePropertyQuery::Maximum,
            StatePropertyQuery::Successor { key } => StatePropertyQuery::Successor {
                key: env.resolve_tuple_ref(key)?,
            },
            StatePropertyQuery::Predecessor { key } => StatePropertyQuery::Predecessor {
                key: env.resolve_tuple_ref(key)?,
            },
            StatePropertyQuery::NonExistenceRange { lower, upper } => {
                StatePropertyQuery::NonExistenceRange {
                    lower: env.resolve_tuple_ref(lower)?,
                    upper: env.resolve_tuple_ref(upper)?,
                }
            }
            StatePropertyQuery::Aggregate { kind } => StatePropertyQuery::Aggregate { kind: *kind },
        })
    }
}

#[derive(Debug)]
struct InlineBuilder {
    next_local: u32,
    locals: Vec<LocalDecl>,
}

impl InlineBuilder {
    fn new(root: &Callable) -> Self {
        Self {
            next_local: root
                .body
                .locals
                .iter()
                .map(|local| local.id.0)
                .max()
                .map_or(0, |max_id| max_id + 1),
            locals: root.body.locals.clone(),
        }
    }

    fn alloc_local(&mut self, ty: ir::TypeRef, symbol: Option<String>) -> LocalId {
        let id = ir::LocalId(self.next_local);
        self.next_local += 1;
        self.locals.push(LocalDecl { id, symbol, ty });
        id
    }

    fn finish_locals(self) -> Vec<LocalDecl> {
        self.locals
    }
}

#[derive(Debug, Clone)]
struct InlineEnv {
    params: BTreeMap<ir::ParamId, ir::ValueRef>,
    locals: BTreeMap<LocalId, LocalId>,
}

impl InlineEnv {
    fn for_root(callable: &Callable) -> Self {
        Self {
            params: callable
                .params
                .iter()
                .map(|param| (param.id, ir::ValueRef::Param(param.id)))
                .collect(),
            locals: callable
                .body
                .locals
                .iter()
                .map(|local| (local.id, local.id))
                .collect(),
        }
    }

    fn for_inline_call(
        callable: &Callable,
        inputs: &ValueTupleRef,
        builder: &mut InlineBuilder,
    ) -> Self {
        let params = callable
            .params
            .iter()
            .zip(inputs.0.iter())
            .map(|(param, value)| (param.id, value.clone()))
            .collect();
        let locals = callable
            .body
            .locals
            .iter()
            .map(|local| {
                (
                    local.id,
                    builder.alloc_local(local.ty, local.symbol.clone()),
                )
            })
            .collect();
        Self { params, locals }
    }

    fn define_local(&mut self, local: LocalId) -> LocalId {
        *self.locals.entry(local).or_insert(local)
    }

    fn resolve_value(&self, value: &ir::ValueRef) -> Result<ir::ValueRef, TabulaError> {
        Ok(match value {
            ir::ValueRef::Literal(value) => ir::ValueRef::Literal(value.clone()),
            ir::ValueRef::Param(param) => self.params.get(param).cloned().ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown inline param {}", param.0))
            })?,
            ir::ValueRef::Context(field) => ir::ValueRef::Context(*field),
            ir::ValueRef::Local(local) => {
                let resolved = self.locals.get(local).copied().ok_or_else(|| {
                    TabulaError::InvalidIr(format!("unknown inline local {}", local.0))
                })?;
                ir::ValueRef::Local(resolved)
            }
            ir::ValueRef::Const(const_id) => ir::ValueRef::Const(*const_id),
        })
    }

    fn resolve_tuple_ref(&self, values: &ValueTupleRef) -> Result<ValueTupleRef, TabulaError> {
        values
            .0
            .iter()
            .map(|value| self.resolve_value(value))
            .collect::<Result<Vec<_>, _>>()
            .map(ir::ValueTupleRef)
    }
}
