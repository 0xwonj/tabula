use std::collections::BTreeMap;

use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID};

use super::{
    CallableLowerCx, LowerRegionKind, invalid, lower_context_field_id, lower_event_id,
    lower_hash_family, single_output, zero_for_type,
};
use crate::error::CompilerError;
use crate::mir;

impl<'a> CallableLowerCx<'a> {
    pub(super) fn new(callable: &'a hir::CallableDecl) -> Self {
        Self {
            callable,
            locals: Vec::new(),
            next_local: 0,
            bindings: BTreeMap::new(),
        }
    }

    pub(super) fn lower_body(&mut self) -> Result<mir::Body, CompilerError> {
        let region = self.lower_region(&self.callable.body.region, LowerRegionKind::Root)?;
        Ok(mir::Body {
            locals: std::mem::take(&mut self.locals),
            region,
        })
    }

    fn lower_region(
        &mut self,
        region: &hir::Region,
        kind: LowerRegionKind,
    ) -> Result<mir::Region, CompilerError> {
        let mut ops = Vec::new();
        for statement in &region.statements {
            self.lower_stmt(statement, &mut ops)?;
        }
        let terminator = match &region.terminator {
            hir::Terminator::Return { values, .. } => mir::Terminator::Return {
                values: ir::ValueTupleRef(
                    values
                        .iter()
                        .map(|value| self.lower_expr_value(value, &mut ops))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            },
            hir::Terminator::Yield { values, .. } => mir::Terminator::Yield {
                values: ir::ValueTupleRef(
                    values
                        .iter()
                        .map(|value| self.lower_expr_value(value, &mut ops))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            },
        };
        match (kind, &terminator) {
            (LowerRegionKind::Root, mir::Terminator::Yield { .. }) => {
                Err(invalid("verified HIR root region unexpectedly yielded"))
            }
            (LowerRegionKind::Nested, mir::Terminator::Return { .. }) => Err(invalid(
                "verified HIR nested control region unexpectedly returned",
            )),
            _ => Ok(mir::Region { ops, terminator }),
        }
    }

    fn lower_nested_region(&mut self, region: &hir::Region) -> Result<mir::Region, CompilerError> {
        let saved_bindings = self.bindings.clone();
        let lowered = self.lower_region(region, LowerRegionKind::Nested);
        self.bindings = saved_bindings;
        lowered
    }

    fn lower_stmt(
        &mut self,
        statement: &hir::Stmt,
        ops: &mut Vec<mir::Op>,
    ) -> Result<(), CompilerError> {
        match statement {
            hir::Stmt::Let(stmt) => {
                let value = self.lower_expr_value(&stmt.value, ops)?;
                self.bindings.insert(stmt.binding.id, value);
            }
            hir::Stmt::StateAssign(stmt) => {
                let key = ir::ValueTupleRef(
                    stmt.target
                        .key
                        .iter()
                        .map(|expr| self.lower_expr_value(expr, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                let value = self.lower_expr_value(&stmt.value, ops)?;
                ops.push(mir::Op::WriteState {
                    table: ir::TableId(stmt.target.table.0),
                    key,
                    field: ir::FieldId(stmt.target.field.0),
                    value,
                });
            }
            hir::Stmt::Assert(stmt) => match stmt {
                hir::AssertStmt::Expr { expr, .. } => {
                    let cond = self.lower_expr_value(expr, ops)?;
                    ops.push(mir::Op::Assert { cond });
                }
                hir::AssertStmt::Relation { relation, args, .. } => {
                    let args = ir::ValueTupleRef(
                        args.iter()
                            .map(|expr| self.lower_expr_value(expr, ops))
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                    ops.push(mir::Op::AssertRelation {
                        relation: ir::RelationId(relation.0),
                        args,
                    });
                }
            },
            hir::Stmt::Emit(stmt) => {
                let args = ir::ValueTupleRef(
                    stmt.args
                        .iter()
                        .map(|expr| self.lower_expr_value(expr, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::EmitEvent {
                    event: lower_event_id(stmt.event),
                    args,
                });
            }
            hir::Stmt::If(stmt) => {
                let cond = self.lower_expr_value(&stmt.cond, ops)?;
                let then_region = self.lower_nested_region(&stmt.then_region)?;
                let else_region = self.lower_nested_region(&stmt.else_region)?;
                ops.push(mir::Op::If {
                    dsts: vec![],
                    cond,
                    then_region,
                    else_region,
                });
            }
            hir::Stmt::Match(stmt) => {
                let scrutinee = self.lower_expr_value(&stmt.scrutinee, ops)?;
                let arms = stmt
                    .arms
                    .iter()
                    .map(|arm| {
                        Ok(mir::MatchArm {
                            pattern: match &arm.pattern {
                                hir::MatchPattern::Literal(value) => {
                                    mir::MatchPattern::Literal(value.clone())
                                }
                            },
                            region: self.lower_nested_region(&arm.region)?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompilerError>>()?;
                let default = stmt
                    .default
                    .as_ref()
                    .map(|region| self.lower_nested_region(region))
                    .transpose()?;
                ops.push(mir::Op::Match {
                    dsts: vec![],
                    scrutinee,
                    arms,
                    default,
                });
            }
            hir::Stmt::Expr(stmt) => self.lower_expr_stmt(&stmt.expr, ops)?,
        }
        Ok(())
    }

    fn lower_expr_stmt(
        &mut self,
        expr: &hir::Expr,
        ops: &mut Vec<mir::Op>,
    ) -> Result<(), CompilerError> {
        match expr {
            hir::Expr::CallFunction(call) => {
                let dsts = call
                    .returns
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| self.alloc_local(*ty, Some(format!("drop_fn_{index}"))))
                    .collect::<Vec<_>>();
                let inputs = ir::ValueTupleRef(
                    call.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::CallFunction {
                    callee: mir::CallableId(call.callee.0),
                    inputs,
                    dsts,
                });
                Ok(())
            }
            hir::Expr::CallCapability(call) => {
                let dsts = call
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| self.alloc_local(*ty, Some(format!("drop_cap_{index}"))))
                    .collect::<Vec<_>>();
                let inputs = ir::ValueTupleRef(
                    call.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::CallCapability {
                    capability: ir::CapabilityId(call.capability.0),
                    inputs,
                    dsts,
                });
                Ok(())
            }
            _ => Err(invalid(
                "unexpected non-call expression statement in verified HIR",
            )),
        }
    }

    fn lower_expr_value(
        &mut self,
        expr: &hir::Expr,
        ops: &mut Vec<mir::Op>,
    ) -> Result<ir::ValueRef, CompilerError> {
        match expr {
            hir::Expr::Literal(expr) => Ok(ir::ValueRef::Literal(expr.value.clone())),
            hir::Expr::Local(expr) => match expr.local {
                hir::LocalRef::Param(param) => Ok(ir::ValueRef::Param(ir::ParamId(param.0))),
                hir::LocalRef::Binding(binding) => self
                    .bindings
                    .get(&binding)
                    .cloned()
                    .ok_or_else(|| invalid("missing lowered binding value")),
            },
            hir::Expr::Context(expr) => {
                Ok(ir::ValueRef::Context(lower_context_field_id(expr.field)))
            }
            hir::Expr::Const(expr) => Ok(ir::ValueRef::Const(ir::ConstId(expr.const_id.0))),
            hir::Expr::TableRead(expr) => {
                let dst_value = self.alloc_local(expr.ty, Some("state_value".into()));
                let dst_present = self.alloc_local(TYPE_BOOL_ID, Some("state_present".into()));
                let key = ir::ValueTupleRef(
                    expr.key
                        .iter()
                        .map(|value| self.lower_expr_value(value, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::ReadState {
                    dst_value,
                    dst_present,
                    table: ir::TableId(expr.table.0),
                    key,
                    field: ir::FieldId(expr.field.0),
                });
                Ok(ir::ValueRef::Local(dst_value))
            }
            hir::Expr::Unary(expr) => {
                let src = self.lower_expr_value(&expr.expr, ops)?;
                let dst = self.alloc_local(expr.ty, None);
                match expr.op {
                    hir::UnaryOp::Not => ops.push(mir::Op::BindValue {
                        dst,
                        value: mir::ValueOp::Not { src },
                    }),
                    hir::UnaryOp::Neg => {
                        ops.push(mir::Op::BindValue {
                            dst,
                            value: mir::ValueOp::Arith {
                                op: ir::ArithOp::Sub,
                                lhs: ir::ValueRef::Literal(zero_for_type(expr.ty)?),
                                rhs: src,
                            },
                        });
                    }
                }
                Ok(ir::ValueRef::Local(dst))
            }
            hir::Expr::Binary(expr) => {
                let lhs = self.lower_expr_value(&expr.lhs, ops)?;
                let rhs = self.lower_expr_value(&expr.rhs, ops)?;
                match expr.op {
                    hir::BinaryOp::Add | hir::BinaryOp::Sub | hir::BinaryOp::Mul => {
                        let dst = self.alloc_local(expr.ty, None);
                        ops.push(mir::Op::BindValue {
                            dst,
                            value: mir::ValueOp::Arith {
                                op: match expr.op {
                                    hir::BinaryOp::Add => ir::ArithOp::Add,
                                    hir::BinaryOp::Sub => ir::ArithOp::Sub,
                                    hir::BinaryOp::Mul => ir::ArithOp::Mul,
                                    _ => unreachable!(),
                                },
                                lhs,
                                rhs,
                            },
                        });
                        Ok(ir::ValueRef::Local(dst))
                    }
                    hir::BinaryOp::Div | hir::BinaryOp::Mod => {
                        let dst_q = self.alloc_local(expr.ty, None);
                        let dst_r = self.alloc_local(expr.ty, None);
                        ops.push(mir::Op::DivMod {
                            dst_q,
                            dst_r,
                            lhs,
                            rhs,
                        });
                        Ok(ir::ValueRef::Local(match expr.op {
                            hir::BinaryOp::Div => dst_q,
                            hir::BinaryOp::Mod => dst_r,
                            _ => unreachable!(),
                        }))
                    }
                    hir::BinaryOp::Eq
                    | hir::BinaryOp::Ne
                    | hir::BinaryOp::Lt
                    | hir::BinaryOp::Le
                    | hir::BinaryOp::Gt
                    | hir::BinaryOp::Ge => {
                        let dst = self.alloc_local(expr.ty, None);
                        ops.push(mir::Op::BindValue {
                            dst,
                            value: mir::ValueOp::Cmp {
                                op: match expr.op {
                                    hir::BinaryOp::Eq => ir::CmpOp::Eq,
                                    hir::BinaryOp::Ne => ir::CmpOp::Ne,
                                    hir::BinaryOp::Lt => ir::CmpOp::Lt,
                                    hir::BinaryOp::Le => ir::CmpOp::Lte,
                                    hir::BinaryOp::Gt => ir::CmpOp::Gt,
                                    hir::BinaryOp::Ge => ir::CmpOp::Gte,
                                    _ => unreachable!(),
                                },
                                lhs,
                                rhs,
                            },
                        });
                        Ok(ir::ValueRef::Local(dst))
                    }
                    hir::BinaryOp::And | hir::BinaryOp::Or => {
                        let dst = self.alloc_local(expr.ty, None);
                        ops.push(mir::Op::BindValue {
                            dst,
                            value: match expr.op {
                                hir::BinaryOp::And => mir::ValueOp::And { lhs, rhs },
                                hir::BinaryOp::Or => mir::ValueOp::Or { lhs, rhs },
                                _ => unreachable!(),
                            },
                        });
                        Ok(ir::ValueRef::Local(dst))
                    }
                }
            }
            hir::Expr::CallFunction(call) => {
                let ty = single_output(&call.returns)?;
                let dst = self.alloc_local(ty, None);
                let inputs = ir::ValueTupleRef(
                    call.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::CallFunction {
                    callee: mir::CallableId(call.callee.0),
                    inputs,
                    dsts: vec![dst],
                });
                Ok(ir::ValueRef::Local(dst))
            }
            hir::Expr::CallCapability(call) => {
                let ty = single_output(&call.outputs)?;
                let dst = self.alloc_local(ty, None);
                let inputs = ir::ValueTupleRef(
                    call.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::CallCapability {
                    capability: ir::CapabilityId(call.capability.0),
                    inputs,
                    dsts: vec![dst],
                });
                Ok(ir::ValueRef::Local(dst))
            }
            hir::Expr::Hash(hash) => {
                let dst = self.alloc_local(TYPE_BYTES32_ID, None);
                let inputs = ir::ValueTupleRef(
                    hash.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::BindValue {
                    dst,
                    value: mir::ValueOp::Hash {
                        family: lower_hash_family(hash.family),
                        inputs,
                    },
                });
                Ok(ir::ValueRef::Local(dst))
            }
            hir::Expr::EvalRelation(eval) => {
                let ty = single_output(&eval.outputs)?;
                let dst = self.alloc_local(ty, None);
                let inputs = ir::ValueTupleRef(
                    eval.args
                        .iter()
                        .map(|arg| self.lower_expr_value(arg, ops))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                ops.push(mir::Op::EvalRelation {
                    relation: ir::RelationId(eval.relation.0),
                    inputs,
                    dsts: vec![dst],
                });
                Ok(ir::ValueRef::Local(dst))
            }
            hir::Expr::Select(select) => {
                let dst = self.alloc_local(select.ty, None);
                let cond = self.lower_expr_value(&select.cond, ops)?;
                let if_true = self.lower_expr_value(&select.if_true, ops)?;
                let if_false = self.lower_expr_value(&select.if_false, ops)?;
                ops.push(mir::Op::BindValue {
                    dst,
                    value: mir::ValueOp::Select {
                        cond,
                        if_true,
                        if_false,
                    },
                });
                Ok(ir::ValueRef::Local(dst))
            }
        }
    }

    fn alloc_local(&mut self, ty: ir::TypeRef, symbol: Option<String>) -> ir::LocalId {
        let local = ir::LocalId(self.next_local);
        self.next_local += 1;
        self.locals.push(mir::LocalDecl {
            id: local,
            symbol,
            ty,
        });
        local
    }
}
