#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_I64_ID, TYPE_U64_ID};

use crate::mir::model::*;
use crate::mir::validate::{VerifiedProgram, verify_program};

pub fn canonicalize_program(program: &VerifiedProgram) -> Result<VerifiedProgram, TabulaError> {
    let raw = program.program();
    let callables = raw
        .callables
        .iter()
        .map(canonicalize_callable)
        .collect::<Result<Vec<_>, _>>()?;
    verify_program(Program {
        program_id: raw.program_id,
        state: raw.state.clone(),
        context: raw.context.clone(),
        const_pool: raw.const_pool.clone(),
        relation_manifest: raw.relation_manifest.clone(),
        capability_manifest: raw.capability_manifest.clone(),
        event_manifest: raw.event_manifest.clone(),
        callables,
    })
}

fn canonicalize_callable(callable: &Callable) -> Result<Callable, TabulaError> {
    let folded = fold_region(&callable.body.region, &FoldEnv::default())?;
    let (pruned, _) = prune_region(&folded)?;
    let needed_locals = collect_region_locals(&pruned);
    let (locals, remap) = densify_locals(&callable.body.locals, &needed_locals);
    let region = remap_region(&pruned, &remap)?;
    Ok(Callable {
        id: callable.id,
        symbol: callable.symbol.clone(),
        kind: callable.kind,
        params: callable.params.clone(),
        returns: callable.returns.clone(),
        body: Body { locals, region },
    })
}

#[derive(Debug, Clone, Default)]
struct FoldEnv {
    substitutions: BTreeMap<LocalId, ValueRef>,
}

impl FoldEnv {
    fn bind(&mut self, local: LocalId, value: ValueRef) -> Result<(), TabulaError> {
        if self.substitutions.insert(local, value).is_some() {
            return Err(TabulaError::InvalidIr(format!(
                "MIR local {} canonicalized more than once",
                local.0
            )));
        }
        Ok(())
    }

    fn resolve_value(&self, value: &ValueRef) -> Result<ValueRef, TabulaError> {
        match value {
            ValueRef::Local(local) => {
                if let Some(subst) = self.substitutions.get(local) {
                    self.resolve_value(subst)
                } else {
                    Ok(ValueRef::Local(*local))
                }
            }
            ValueRef::Literal(value) => Ok(ValueRef::Literal(value.clone())),
            ValueRef::Param(id) => Ok(ValueRef::Param(*id)),
            ValueRef::Context(id) => Ok(ValueRef::Context(*id)),
            ValueRef::Const(id) => Ok(ValueRef::Const(*id)),
        }
    }

    fn resolve_tuple(&self, values: &ValueTupleRef) -> Result<ValueTupleRef, TabulaError> {
        Ok(ir::ValueTupleRef(
            values
                .0
                .iter()
                .map(|value| self.resolve_value(value))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

fn fold_region(region: &Region, env: &FoldEnv) -> Result<Region, TabulaError> {
    let mut env = env.clone();
    let mut ops = Vec::new();
    for op in &region.ops {
        match op {
            Op::BindValue { dst, value } => match simplify_value_op(value, &env)? {
                SimplifiedValueOp::Alias(value) => env.bind(*dst, value)?,
                SimplifiedValueOp::Keep(value) => ops.push(Op::BindValue { dst: *dst, value }),
            },
            Op::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => ops.push(Op::DivMod {
                dst_q: *dst_q,
                dst_r: *dst_r,
                lhs: env.resolve_value(lhs)?,
                rhs: env.resolve_value(rhs)?,
            }),
            Op::ReadState {
                dst_value,
                dst_present,
                table,
                key,
                field,
            } => ops.push(Op::ReadState {
                dst_value: *dst_value,
                dst_present: *dst_present,
                table: *table,
                key: env.resolve_tuple(key)?,
                field: *field,
            }),
            Op::WriteState {
                table,
                key,
                field,
                value,
            } => ops.push(Op::WriteState {
                table: *table,
                key: env.resolve_tuple(key)?,
                field: *field,
                value: env.resolve_value(value)?,
            }),
            Op::DeleteState { table, key, field } => ops.push(Op::DeleteState {
                table: *table,
                key: env.resolve_tuple(key)?,
                field: *field,
            }),
            Op::ReadStateProperty {
                dsts,
                table,
                field,
                query,
            } => ops.push(Op::ReadStateProperty {
                dsts: dsts.clone(),
                table: *table,
                field: *field,
                query: fold_property_query(query, &env)?,
            }),
            Op::Assert { cond } => ops.push(Op::Assert {
                cond: env.resolve_value(cond)?,
            }),
            Op::AssertRelation { relation, args } => ops.push(Op::AssertRelation {
                relation: *relation,
                args: env.resolve_tuple(args)?,
            }),
            Op::EvalRelation {
                relation,
                inputs,
                dsts,
            } => ops.push(Op::EvalRelation {
                relation: *relation,
                inputs: env.resolve_tuple(inputs)?,
                dsts: dsts.clone(),
            }),
            Op::CallCapability {
                capability,
                inputs,
                dsts,
            } => ops.push(Op::CallCapability {
                capability: *capability,
                inputs: env.resolve_tuple(inputs)?,
                dsts: dsts.clone(),
            }),
            Op::CallFunction {
                callee,
                inputs,
                dsts,
            } => ops.push(Op::CallFunction {
                callee: *callee,
                inputs: env.resolve_tuple(inputs)?,
                dsts: dsts.clone(),
            }),
            Op::EmitEvent { event, args } => ops.push(Op::EmitEvent {
                event: *event,
                args: env.resolve_tuple(args)?,
            }),
            Op::If {
                dsts,
                cond,
                then_region,
                else_region,
            } => {
                let cond = env.resolve_value(cond)?;
                let then_region = fold_region(then_region, &env)?;
                let else_region = fold_region(else_region, &env)?;
                ops.push(Op::If {
                    dsts: dsts.clone(),
                    cond,
                    then_region,
                    else_region,
                });
            }
            Op::Match {
                dsts,
                scrutinee,
                arms,
                default,
            } => {
                let scrutinee = env.resolve_value(scrutinee)?;
                let arms = arms
                    .iter()
                    .map(|arm| {
                        Ok(MatchArm {
                            pattern: arm.pattern.clone(),
                            region: fold_region(&arm.region, &env)?,
                        })
                    })
                    .collect::<Result<Vec<_>, TabulaError>>()?;
                let default = default
                    .as_ref()
                    .map(|region| fold_region(region, &env))
                    .transpose()?;
                ops.push(Op::Match {
                    dsts: dsts.clone(),
                    scrutinee,
                    arms,
                    default,
                });
            }
        }
    }
    let terminator = match &region.terminator {
        Terminator::Yield { values } => Terminator::Yield {
            values: env.resolve_tuple(values)?,
        },
        Terminator::Return { values } => Terminator::Return {
            values: env.resolve_tuple(values)?,
        },
    };
    Ok(Region { ops, terminator })
}

enum SimplifiedValueOp {
    Alias(ValueRef),
    Keep(ValueOp),
}

fn simplify_value_op(value: &ValueOp, env: &FoldEnv) -> Result<SimplifiedValueOp, TabulaError> {
    Ok(match value {
        ValueOp::Arith { op, lhs, rhs } => {
            let lhs = env.resolve_value(lhs)?;
            let rhs = env.resolve_value(rhs)?;
            if let Some(value) = fold_arith(*op, &lhs, &rhs)? {
                SimplifiedValueOp::Alias(ValueRef::Literal(value))
            } else {
                SimplifiedValueOp::Keep(ValueOp::Arith { op: *op, lhs, rhs })
            }
        }
        ValueOp::Cmp { op, lhs, rhs } => {
            let lhs = env.resolve_value(lhs)?;
            let rhs = env.resolve_value(rhs)?;
            if let Some(value) = fold_cmp(*op, &lhs, &rhs)? {
                SimplifiedValueOp::Alias(ValueRef::Literal(value))
            } else {
                SimplifiedValueOp::Keep(ValueOp::Cmp { op: *op, lhs, rhs })
            }
        }
        ValueOp::Not { src } => {
            let src = env.resolve_value(src)?;
            if let Some(value) = fold_not(&src)? {
                SimplifiedValueOp::Alias(ValueRef::Literal(value))
            } else {
                SimplifiedValueOp::Keep(ValueOp::Not { src })
            }
        }
        ValueOp::And { lhs, rhs } => {
            let lhs = env.resolve_value(lhs)?;
            let rhs = env.resolve_value(rhs)?;
            if let Some(value) = fold_bool_binop(BoolBinOp::And, &lhs, &rhs)? {
                SimplifiedValueOp::Alias(ValueRef::Literal(value))
            } else {
                SimplifiedValueOp::Keep(ValueOp::And { lhs, rhs })
            }
        }
        ValueOp::Or { lhs, rhs } => {
            let lhs = env.resolve_value(lhs)?;
            let rhs = env.resolve_value(rhs)?;
            if let Some(value) = fold_bool_binop(BoolBinOp::Or, &lhs, &rhs)? {
                SimplifiedValueOp::Alias(ValueRef::Literal(value))
            } else {
                SimplifiedValueOp::Keep(ValueOp::Or { lhs, rhs })
            }
        }
        ValueOp::Select {
            cond,
            if_true,
            if_false,
        } => {
            let cond = env.resolve_value(cond)?;
            let if_true = env.resolve_value(if_true)?;
            let if_false = env.resolve_value(if_false)?;
            if if_true == if_false {
                SimplifiedValueOp::Alias(if_true)
            } else if let Some(cond) = literal_bool(&cond)? {
                SimplifiedValueOp::Alias(if cond { if_true } else { if_false })
            } else {
                SimplifiedValueOp::Keep(ValueOp::Select {
                    cond,
                    if_true,
                    if_false,
                })
            }
        }
        ValueOp::Hash { family, inputs } => SimplifiedValueOp::Keep(ValueOp::Hash {
            family: *family,
            inputs: env.resolve_tuple(inputs)?,
        }),
    })
}

fn fold_property_query(
    query: &StatePropertyQuery,
    env: &FoldEnv,
) -> Result<StatePropertyQuery, TabulaError> {
    Ok(match query {
        StatePropertyQuery::Minimum => StatePropertyQuery::Minimum,
        StatePropertyQuery::Maximum => StatePropertyQuery::Maximum,
        StatePropertyQuery::Aggregate { kind } => StatePropertyQuery::Aggregate { kind: *kind },
        StatePropertyQuery::Successor { key } => StatePropertyQuery::Successor {
            key: env.resolve_tuple(key)?,
        },
        StatePropertyQuery::Predecessor { key } => StatePropertyQuery::Predecessor {
            key: env.resolve_tuple(key)?,
        },
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            StatePropertyQuery::NonExistenceRange {
                lower: env.resolve_tuple(lower)?,
                upper: env.resolve_tuple(upper)?,
            }
        }
    })
}

fn prune_region(region: &Region) -> Result<(Region, BTreeSet<LocalId>), TabulaError> {
    let mut live = locals_in_tuple(region.terminator.values());
    let mut kept = Vec::with_capacity(region.ops.len());

    for op in region.ops.iter().rev() {
        match op {
            Op::BindValue { dst, value } => {
                if !live.contains(dst) {
                    continue;
                }
                live.remove(dst);
                live.extend(locals_in_value_op(value));
                kept.push(op.clone());
            }
            Op::If {
                dsts,
                cond,
                then_region,
                else_region,
            } => {
                let (then_region, then_live) = prune_region(then_region)?;
                let (else_region, else_live) = prune_region(else_region)?;
                for dst in dsts {
                    live.remove(dst);
                }
                live.extend(locals_in_value(cond));
                live.extend(then_live);
                live.extend(else_live);
                kept.push(Op::If {
                    dsts: dsts.clone(),
                    cond: cond.clone(),
                    then_region,
                    else_region,
                });
            }
            Op::Match {
                dsts,
                scrutinee,
                arms,
                default,
            } => {
                let mut arm_live = BTreeSet::new();
                let mut pruned_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let (region, live_in) = prune_region(&arm.region)?;
                    arm_live.extend(live_in);
                    pruned_arms.push(MatchArm {
                        pattern: arm.pattern.clone(),
                        region,
                    });
                }
                let pruned_default = if let Some(default) = default {
                    let (region, live_in) = prune_region(default)?;
                    arm_live.extend(live_in);
                    Some(region)
                } else {
                    None
                };
                for dst in dsts {
                    live.remove(dst);
                }
                live.extend(locals_in_value(scrutinee));
                live.extend(arm_live);
                kept.push(Op::Match {
                    dsts: dsts.clone(),
                    scrutinee: scrutinee.clone(),
                    arms: pruned_arms,
                    default: pruned_default,
                });
            }
            _ => {
                for dst in op.defines_locals() {
                    live.remove(&dst);
                }
                live.extend(locals_in_op(op));
                kept.push(op.clone());
            }
        }
    }

    kept.reverse();
    Ok((
        Region {
            ops: kept,
            terminator: region.terminator.clone(),
        },
        live,
    ))
}

fn densify_locals(
    locals: &[LocalDecl],
    needed: &BTreeSet<LocalId>,
) -> (Vec<LocalDecl>, BTreeMap<LocalId, LocalId>) {
    let mut remap = BTreeMap::new();
    let mut dense_locals = Vec::new();
    let mut next = 0u32;
    for local in locals {
        if needed.contains(&local.id) {
            let new_id = ir::LocalId(next);
            next = next.saturating_add(1);
            remap.insert(local.id, new_id);
            dense_locals.push(LocalDecl {
                id: new_id,
                symbol: local.symbol.clone(),
                ty: local.ty,
            });
        }
    }
    (dense_locals, remap)
}

fn remap_region(
    region: &Region,
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<Region, TabulaError> {
    let ops = region
        .ops
        .iter()
        .map(|op| remap_op(op, remap))
        .collect::<Result<Vec<_>, _>>()?;
    let terminator = match &region.terminator {
        Terminator::Yield { values } => Terminator::Yield {
            values: remap_tuple(values, remap)?,
        },
        Terminator::Return { values } => Terminator::Return {
            values: remap_tuple(values, remap)?,
        },
    };
    Ok(Region { ops, terminator })
}

fn remap_op(op: &Op, remap: &BTreeMap<LocalId, LocalId>) -> Result<Op, TabulaError> {
    Ok(match op {
        Op::BindValue { dst, value } => Op::BindValue {
            dst: remap_local(*dst, remap)?,
            value: remap_value_op(value, remap)?,
        },
        Op::DivMod {
            dst_q,
            dst_r,
            lhs,
            rhs,
        } => Op::DivMod {
            dst_q: remap_local(*dst_q, remap)?,
            dst_r: remap_local(*dst_r, remap)?,
            lhs: remap_value(lhs, remap)?,
            rhs: remap_value(rhs, remap)?,
        },
        Op::ReadState {
            dst_value,
            dst_present,
            table,
            key,
            field,
        } => Op::ReadState {
            dst_value: remap_local(*dst_value, remap)?,
            dst_present: remap_local(*dst_present, remap)?,
            table: *table,
            key: remap_tuple(key, remap)?,
            field: *field,
        },
        Op::WriteState {
            table,
            key,
            field,
            value,
        } => Op::WriteState {
            table: *table,
            key: remap_tuple(key, remap)?,
            field: *field,
            value: remap_value(value, remap)?,
        },
        Op::DeleteState { table, key, field } => Op::DeleteState {
            table: *table,
            key: remap_tuple(key, remap)?,
            field: *field,
        },
        Op::ReadStateProperty {
            dsts,
            table,
            field,
            query,
        } => Op::ReadStateProperty {
            dsts: remap_locals(dsts, remap)?,
            table: *table,
            field: *field,
            query: remap_property_query(query, remap)?,
        },
        Op::Assert { cond } => Op::Assert {
            cond: remap_value(cond, remap)?,
        },
        Op::AssertRelation { relation, args } => Op::AssertRelation {
            relation: *relation,
            args: remap_tuple(args, remap)?,
        },
        Op::EvalRelation {
            relation,
            inputs,
            dsts,
        } => Op::EvalRelation {
            relation: *relation,
            inputs: remap_tuple(inputs, remap)?,
            dsts: remap_locals(dsts, remap)?,
        },
        Op::CallCapability {
            capability,
            inputs,
            dsts,
        } => Op::CallCapability {
            capability: *capability,
            inputs: remap_tuple(inputs, remap)?,
            dsts: remap_locals(dsts, remap)?,
        },
        Op::CallFunction {
            callee,
            inputs,
            dsts,
        } => Op::CallFunction {
            callee: *callee,
            inputs: remap_tuple(inputs, remap)?,
            dsts: remap_locals(dsts, remap)?,
        },
        Op::EmitEvent { event, args } => Op::EmitEvent {
            event: *event,
            args: remap_tuple(args, remap)?,
        },
        Op::If {
            dsts,
            cond,
            then_region,
            else_region,
        } => Op::If {
            dsts: remap_locals(dsts, remap)?,
            cond: remap_value(cond, remap)?,
            then_region: remap_region(then_region, remap)?,
            else_region: remap_region(else_region, remap)?,
        },
        Op::Match {
            dsts,
            scrutinee,
            arms,
            default,
        } => Op::Match {
            dsts: remap_locals(dsts, remap)?,
            scrutinee: remap_value(scrutinee, remap)?,
            arms: arms
                .iter()
                .map(|arm| {
                    Ok(MatchArm {
                        pattern: arm.pattern.clone(),
                        region: remap_region(&arm.region, remap)?,
                    })
                })
                .collect::<Result<Vec<_>, TabulaError>>()?,
            default: default
                .as_ref()
                .map(|region| remap_region(region, remap))
                .transpose()?,
        },
    })
}

fn remap_value_op(
    value: &ValueOp,
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<ValueOp, TabulaError> {
    Ok(match value {
        ValueOp::Arith { op, lhs, rhs } => ValueOp::Arith {
            op: *op,
            lhs: remap_value(lhs, remap)?,
            rhs: remap_value(rhs, remap)?,
        },
        ValueOp::Cmp { op, lhs, rhs } => ValueOp::Cmp {
            op: *op,
            lhs: remap_value(lhs, remap)?,
            rhs: remap_value(rhs, remap)?,
        },
        ValueOp::Not { src } => ValueOp::Not {
            src: remap_value(src, remap)?,
        },
        ValueOp::And { lhs, rhs } => ValueOp::And {
            lhs: remap_value(lhs, remap)?,
            rhs: remap_value(rhs, remap)?,
        },
        ValueOp::Or { lhs, rhs } => ValueOp::Or {
            lhs: remap_value(lhs, remap)?,
            rhs: remap_value(rhs, remap)?,
        },
        ValueOp::Select {
            cond,
            if_true,
            if_false,
        } => ValueOp::Select {
            cond: remap_value(cond, remap)?,
            if_true: remap_value(if_true, remap)?,
            if_false: remap_value(if_false, remap)?,
        },
        ValueOp::Hash { family, inputs } => ValueOp::Hash {
            family: *family,
            inputs: remap_tuple(inputs, remap)?,
        },
    })
}

fn remap_property_query(
    query: &StatePropertyQuery,
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<StatePropertyQuery, TabulaError> {
    Ok(match query {
        StatePropertyQuery::Minimum => StatePropertyQuery::Minimum,
        StatePropertyQuery::Maximum => StatePropertyQuery::Maximum,
        StatePropertyQuery::Aggregate { kind } => StatePropertyQuery::Aggregate { kind: *kind },
        StatePropertyQuery::Successor { key } => StatePropertyQuery::Successor {
            key: remap_tuple(key, remap)?,
        },
        StatePropertyQuery::Predecessor { key } => StatePropertyQuery::Predecessor {
            key: remap_tuple(key, remap)?,
        },
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            StatePropertyQuery::NonExistenceRange {
                lower: remap_tuple(lower, remap)?,
                upper: remap_tuple(upper, remap)?,
            }
        }
    })
}

fn remap_value(
    value: &ValueRef,
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<ValueRef, TabulaError> {
    Ok(match value {
        ValueRef::Local(local) => ValueRef::Local(remap_local(*local, remap)?),
        ValueRef::Literal(value) => ValueRef::Literal(value.clone()),
        ValueRef::Param(id) => ValueRef::Param(*id),
        ValueRef::Context(id) => ValueRef::Context(*id),
        ValueRef::Const(id) => ValueRef::Const(*id),
    })
}

fn remap_tuple(
    values: &ValueTupleRef,
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<ValueTupleRef, TabulaError> {
    Ok(ir::ValueTupleRef(
        values
            .0
            .iter()
            .map(|value| remap_value(value, remap))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn remap_locals(
    locals: &[LocalId],
    remap: &BTreeMap<LocalId, LocalId>,
) -> Result<Vec<LocalId>, TabulaError> {
    locals
        .iter()
        .map(|local| remap_local(*local, remap))
        .collect()
}

fn remap_local(local: LocalId, remap: &BTreeMap<LocalId, LocalId>) -> Result<LocalId, TabulaError> {
    remap.get(&local).copied().ok_or_else(|| {
        TabulaError::InvalidIr(format!("missing dense remap for MIR local {}", local.0))
    })
}

fn collect_region_locals(region: &Region) -> BTreeSet<LocalId> {
    let mut locals = locals_in_tuple(region.terminator.values());
    for op in &region.ops {
        locals.extend(op.defines_locals());
        locals.extend(locals_in_op(op));
        for nested in op.nested_regions() {
            locals.extend(collect_region_locals(nested));
        }
    }
    locals
}

fn locals_in_op(op: &Op) -> BTreeSet<LocalId> {
    match op {
        Op::BindValue { value, .. } => locals_in_value_op(value),
        Op::DivMod { lhs, rhs, .. } => union_sets([locals_in_value(lhs), locals_in_value(rhs)]),
        Op::ReadState { key, .. } | Op::DeleteState { key, .. } => locals_in_tuple(key),
        Op::WriteState { key, value, .. } => {
            union_sets([locals_in_tuple(key), locals_in_value(value)])
        }
        Op::ReadStateProperty { query, .. } => locals_in_property_query(query),
        Op::Assert { cond } => locals_in_value(cond),
        Op::AssertRelation { args, .. } | Op::EmitEvent { args, .. } => locals_in_tuple(args),
        Op::EvalRelation { inputs, .. }
        | Op::CallCapability { inputs, .. }
        | Op::CallFunction { inputs, .. } => locals_in_tuple(inputs),
        Op::If { cond, .. } => locals_in_value(cond),
        Op::Match { scrutinee, .. } => locals_in_value(scrutinee),
    }
}

fn locals_in_value_op(value: &ValueOp) -> BTreeSet<LocalId> {
    match value {
        ValueOp::Arith { lhs, rhs, .. }
        | ValueOp::Cmp { lhs, rhs, .. }
        | ValueOp::And { lhs, rhs }
        | ValueOp::Or { lhs, rhs } => union_sets([locals_in_value(lhs), locals_in_value(rhs)]),
        ValueOp::Not { src } => locals_in_value(src),
        ValueOp::Select {
            cond,
            if_true,
            if_false,
        } => union_sets([
            locals_in_value(cond),
            locals_in_value(if_true),
            locals_in_value(if_false),
        ]),
        ValueOp::Hash { inputs, .. } => locals_in_tuple(inputs),
    }
}

fn locals_in_property_query(query: &StatePropertyQuery) -> BTreeSet<LocalId> {
    match query {
        StatePropertyQuery::Minimum
        | StatePropertyQuery::Maximum
        | StatePropertyQuery::Aggregate { .. } => BTreeSet::new(),
        StatePropertyQuery::Successor { key } | StatePropertyQuery::Predecessor { key } => {
            locals_in_tuple(key)
        }
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            union_sets([locals_in_tuple(lower), locals_in_tuple(upper)])
        }
    }
}

fn locals_in_tuple(values: &ValueTupleRef) -> BTreeSet<LocalId> {
    values.0.iter().flat_map(locals_in_value).collect()
}

fn locals_in_value(value: &ValueRef) -> BTreeSet<LocalId> {
    match value {
        ValueRef::Local(local) => BTreeSet::from([*local]),
        ValueRef::Literal(_) | ValueRef::Param(_) | ValueRef::Context(_) | ValueRef::Const(_) => {
            BTreeSet::new()
        }
    }
}

fn union_sets<const N: usize>(sets: [BTreeSet<LocalId>; N]) -> BTreeSet<LocalId> {
    let mut merged = BTreeSet::new();
    for set in sets {
        merged.extend(set);
    }
    merged
}

#[derive(Clone, Copy)]
enum BoolBinOp {
    And,
    Or,
}

fn fold_arith(
    op: ArithOp,
    lhs: &ValueRef,
    rhs: &ValueRef,
) -> Result<Option<PortableValue>, TabulaError> {
    let (ValueRef::Literal(lhs), ValueRef::Literal(rhs)) = (lhs, rhs) else {
        return Ok(None);
    };
    if lhs.type_id() != rhs.type_id() {
        return Ok(None);
    }
    if lhs.type_id() == TYPE_U64_ID {
        let lhs = decode_u64(lhs)?;
        let rhs = decode_u64(rhs)?;
        let value = match op {
            ArithOp::Add => lhs.checked_add(rhs),
            ArithOp::Sub => lhs.checked_sub(rhs),
            ArithOp::Mul => lhs.checked_mul(rhs),
        };
        Ok(value.map(portable_u64))
    } else if lhs.type_id() == TYPE_I64_ID {
        let lhs = decode_i64(lhs)?;
        let rhs = decode_i64(rhs)?;
        let value = match op {
            ArithOp::Add => lhs.checked_add(rhs),
            ArithOp::Sub => lhs.checked_sub(rhs),
            ArithOp::Mul => lhs.checked_mul(rhs),
        };
        Ok(value.map(portable_i64))
    } else {
        Ok(None)
    }
}

fn fold_cmp(
    op: CmpOp,
    lhs: &ValueRef,
    rhs: &ValueRef,
) -> Result<Option<PortableValue>, TabulaError> {
    let (ValueRef::Literal(lhs), ValueRef::Literal(rhs)) = (lhs, rhs) else {
        return Ok(None);
    };
    if lhs.type_id() != rhs.type_id() {
        return Ok(None);
    }
    let value = match op {
        CmpOp::Eq => lhs == rhs,
        CmpOp::Ne => lhs != rhs,
        CmpOp::Lt | CmpOp::Lte | CmpOp::Gt | CmpOp::Gte => {
            if lhs.type_id() == TYPE_U64_ID {
                let lhs = decode_u64(lhs)?;
                let rhs = decode_u64(rhs)?;
                cmp_ordering(op, lhs.cmp(&rhs))
            } else if lhs.type_id() == TYPE_I64_ID {
                let lhs = decode_i64(lhs)?;
                let rhs = decode_i64(rhs)?;
                cmp_ordering(op, lhs.cmp(&rhs))
            } else {
                return Ok(None);
            }
        }
    };
    Ok(Some(portable_bool(value)))
}

fn cmp_ordering(op: CmpOp, ordering: std::cmp::Ordering) -> bool {
    match op {
        CmpOp::Eq => ordering == std::cmp::Ordering::Equal,
        CmpOp::Ne => ordering != std::cmp::Ordering::Equal,
        CmpOp::Lt => ordering == std::cmp::Ordering::Less,
        CmpOp::Lte => ordering != std::cmp::Ordering::Greater,
        CmpOp::Gt => ordering == std::cmp::Ordering::Greater,
        CmpOp::Gte => ordering != std::cmp::Ordering::Less,
    }
}

fn fold_not(src: &ValueRef) -> Result<Option<PortableValue>, TabulaError> {
    let Some(value) = literal_bool(src)? else {
        return Ok(None);
    };
    Ok(Some(portable_bool(!value)))
}

fn fold_bool_binop(
    op: BoolBinOp,
    lhs: &ValueRef,
    rhs: &ValueRef,
) -> Result<Option<PortableValue>, TabulaError> {
    let (Some(lhs), Some(rhs)) = (literal_bool(lhs)?, literal_bool(rhs)?) else {
        return Ok(None);
    };
    Ok(Some(portable_bool(match op {
        BoolBinOp::And => lhs && rhs,
        BoolBinOp::Or => lhs || rhs,
    })))
}

fn literal_bool(value: &ValueRef) -> Result<Option<bool>, TabulaError> {
    let ValueRef::Literal(value) = value else {
        return Ok(None);
    };
    if value.type_id() != TYPE_BOOL_ID {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice::<bool>(value.payload()).map_err(
        |err| {
            TabulaError::InvalidIr(format!(
                "failed to decode bool literal during canonicalize: {err}"
            ))
        },
    )?))
}

fn decode_u64(value: &PortableValue) -> Result<u64, TabulaError> {
    borsh::from_slice::<u64>(value.payload()).map_err(|err| {
        TabulaError::InvalidIr(format!(
            "failed to decode u64 literal during canonicalize: {err}"
        ))
    })
}

fn decode_i64(value: &PortableValue) -> Result<i64, TabulaError> {
    borsh::from_slice::<i64>(value.payload()).map_err(|err| {
        TabulaError::InvalidIr(format!(
            "failed to decode i64 literal during canonicalize: {err}"
        ))
    })
}

fn portable_bool(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, borsh::to_vec(&value).expect("bool literal"))
}

fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, borsh::to_vec(&value).expect("u64 literal"))
}

fn portable_i64(value: i64) -> PortableValue {
    PortableValue::new(TYPE_I64_ID, borsh::to_vec(&value).expect("i64 literal"))
}
