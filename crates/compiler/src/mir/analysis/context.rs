use super::super::model::{Op, StatePropertyQuery, ValueOp, ValueRef, ValueTupleRef};
use super::ContextDemandSummary;

pub(super) fn collect_context_demands_from_op(op: &Op, demands: &mut ContextDemandSummary) {
    match op {
        Op::BindValue { value, .. } => collect_context_demands_from_value_op(value, demands),
        Op::DivMod { lhs, rhs, .. } => {
            collect_context_demands_from_value(lhs, demands);
            collect_context_demands_from_value(rhs, demands);
        }
        Op::ReadState { key, .. }
        | Op::AssertRelation { args: key, .. }
        | Op::EmitEvent { args: key, .. } => collect_context_demands_from_tuple(key, demands),
        Op::WriteState { key, value, .. } => {
            collect_context_demands_from_tuple(key, demands);
            collect_context_demands_from_value(value, demands);
        }
        Op::DeleteState { key, .. } => collect_context_demands_from_tuple(key, demands),
        Op::ReadStateProperty { query, .. } => {
            collect_context_demands_from_state_property_query(query, demands);
        }
        Op::Assert { cond } => collect_context_demands_from_value(cond, demands),
        Op::EvalRelation { inputs, .. }
        | Op::CallCapability { inputs, .. }
        | Op::CallFunction { inputs, .. } => {
            collect_context_demands_from_tuple(inputs, demands);
        }
        Op::If { cond, .. } => collect_context_demands_from_value(cond, demands),
        Op::Match { scrutinee, .. } => collect_context_demands_from_value(scrutinee, demands),
    }
}

fn collect_context_demands_from_state_property_query(
    query: &StatePropertyQuery,
    demands: &mut ContextDemandSummary,
) {
    match query {
        StatePropertyQuery::Minimum | StatePropertyQuery::Maximum => {}
        StatePropertyQuery::Successor { key } | StatePropertyQuery::Predecessor { key } => {
            collect_context_demands_from_tuple(key, demands);
        }
        StatePropertyQuery::NonExistenceRange { lower, upper } => {
            collect_context_demands_from_tuple(lower, demands);
            collect_context_demands_from_tuple(upper, demands);
        }
        StatePropertyQuery::Aggregate { .. } => {}
    }
}

fn collect_context_demands_from_value_op(value: &ValueOp, demands: &mut ContextDemandSummary) {
    match value {
        ValueOp::Arith { lhs, rhs, .. }
        | ValueOp::Cmp { lhs, rhs, .. }
        | ValueOp::And { lhs, rhs }
        | ValueOp::Or { lhs, rhs } => {
            collect_context_demands_from_value(lhs, demands);
            collect_context_demands_from_value(rhs, demands);
        }
        ValueOp::Not { src } => collect_context_demands_from_value(src, demands),
        ValueOp::Select {
            cond,
            if_true,
            if_false,
        } => {
            collect_context_demands_from_value(cond, demands);
            collect_context_demands_from_value(if_true, demands);
            collect_context_demands_from_value(if_false, demands);
        }
        ValueOp::Hash { inputs, .. } => collect_context_demands_from_tuple(inputs, demands),
    }
}

pub(super) fn collect_context_demands_from_tuple(
    values: &ValueTupleRef,
    demands: &mut ContextDemandSummary,
) {
    for value in &values.0 {
        collect_context_demands_from_value(value, demands);
    }
}

fn collect_context_demands_from_value(value: &ValueRef, demands: &mut ContextDemandSummary) {
    if let ValueRef::Context(field) = value {
        demands.fields.insert(*field);
    }
}
