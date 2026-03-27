#![allow(missing_docs)]

use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID};

use super::consts::{
    build_literal_value, convert_binary_op, convert_unary_op, literal_type, single_output_ty,
    single_segment,
};
#[allow(clippy::wildcard_imports)]
use super::*;
use crate::error::{FrontendError, FrontendErrorKind};

impl TypedExpr {
    pub(super) fn require_value(
        self,
        message: &'static str,
    ) -> Result<(hir::Expr, hir::TypeRef), FrontendError> {
        match self.ty {
            Some(ty) => Ok((self.expr, ty)),
            None => Err(FrontendError::new(
                FrontendErrorKind::TypeMismatch,
                self.expr.span(),
                message,
            )),
        }
    }
}

impl<'a> BodyBuildCx<'a> {
    pub(super) fn build_expr(&mut self, expr: &ast::Expr) -> Result<TypedExpr, FrontendError> {
        match expr {
            ast::Expr::Literal(literal) => Ok(TypedExpr {
                expr: hir::Expr::Literal(hir::LiteralExpr {
                    value: build_literal_value(&literal.kind, None, literal.span)?,
                    span: literal.span,
                }),
                ty: Some(literal_type(&literal.kind)),
            }),
            ast::Expr::Name(path) => self.build_name_expr(path),
            ast::Expr::Unary(unary) => self.build_unary_expr(unary),
            ast::Expr::Binary(binary) => self.build_binary_expr(binary),
            ast::Expr::Call(call) => self.build_call_expr(call),
            ast::Expr::TableRead(read) => self.build_table_read(read),
            ast::Expr::EvalRelation(eval) => self.build_eval_relation(eval),
            ast::Expr::Select(select) => self.build_select_expr(select),
        }
    }

    fn build_name_expr(&self, path: &ast::IdentPath) -> Result<TypedExpr, FrontendError> {
        let symbol = single_segment(path, path.span)?;
        if let Some(binding) = self.bindings.get(symbol) {
            return Ok(TypedExpr {
                expr: hir::Expr::Local(hir::LocalRefExpr {
                    local: hir::LocalRef::Binding(binding.id),
                    span: path.span,
                }),
                ty: Some(binding.ty),
            });
        }
        if let Some(param) = self.params.iter().find(|param| param.symbol == symbol) {
            return Ok(TypedExpr {
                expr: hir::Expr::Local(hir::LocalRefExpr {
                    local: hir::LocalRef::Param(param.id),
                    span: path.span,
                }),
                ty: Some(param.ty),
            });
        }
        if let Some(field) = self.context_fields.get(symbol) {
            return Ok(TypedExpr {
                expr: hir::Expr::Context(hir::ContextRefExpr {
                    field: field.id,
                    span: path.span,
                }),
                ty: Some(field.ty),
            });
        }
        if let Some(const_decl) = self.consts.get(symbol) {
            return Ok(TypedExpr {
                expr: hir::Expr::Const(hir::ConstRefExpr {
                    const_id: const_decl.id,
                    span: path.span,
                }),
                ty: Some(const_decl.ty),
            });
        }
        Err(FrontendError::new(
            FrontendErrorKind::UndefinedSymbol,
            path.span,
            format!("unresolved identifier {}", path.as_string()),
        ))
    }

    fn build_unary_expr(&mut self, unary: &ast::UnaryExpr) -> Result<TypedExpr, FrontendError> {
        let typed = self.build_expr(&unary.expr)?;
        let (expr, _) = typed.require_value("unary operator requires value operand")?;
        let out_ty = match unary.op {
            ast::UnaryOp::Not => TYPE_BOOL_ID,
            ast::UnaryOp::Neg => TYPE_I64_ID,
        };
        Ok(TypedExpr {
            expr: hir::Expr::Unary(hir::UnaryExpr {
                op: convert_unary_op(unary.op),
                expr: Box::new(expr),
                ty: out_ty,
                span: unary.span,
            }),
            ty: Some(out_ty),
        })
    }

    fn build_binary_expr(&mut self, binary: &ast::BinaryExpr) -> Result<TypedExpr, FrontendError> {
        let lhs = self.build_expr(&binary.lhs)?;
        let rhs = self.build_expr(&binary.rhs)?;
        let (lhs_expr, lhs_ty) = lhs.require_value("binary operator requires value operands")?;
        let (rhs_expr, _) = rhs.require_value("binary operator requires value operands")?;
        let out_ty = match binary.op {
            ast::BinaryOp::And
            | ast::BinaryOp::Or
            | ast::BinaryOp::Eq
            | ast::BinaryOp::Ne
            | ast::BinaryOp::Lt
            | ast::BinaryOp::Le
            | ast::BinaryOp::Gt
            | ast::BinaryOp::Ge => TYPE_BOOL_ID,
            ast::BinaryOp::Add
            | ast::BinaryOp::Sub
            | ast::BinaryOp::Mul
            | ast::BinaryOp::Div
            | ast::BinaryOp::Mod => lhs_ty,
        };
        Ok(TypedExpr {
            expr: hir::Expr::Binary(hir::BinaryExpr {
                op: convert_binary_op(binary.op),
                lhs: Box::new(lhs_expr),
                rhs: Box::new(rhs_expr),
                ty: out_ty,
                span: binary.span,
            }),
            ty: Some(out_ty),
        })
    }

    fn build_call_expr(&mut self, call: &ast::CallExpr) -> Result<TypedExpr, FrontendError> {
        let symbol = single_segment(&call.callee, call.span)?;
        if let Some(callable) = self.callables.get(symbol) {
            let args = self.build_exprs_as_values(&call.args)?;
            return Ok(TypedExpr {
                expr: hir::Expr::CallFunction(hir::CallFunctionExpr {
                    callee: callable.id,
                    args: args.into_iter().map(|(expr, _)| expr).collect(),
                    returns: callable.returns.clone(),
                    span: call.span,
                }),
                ty: single_output_ty(&callable.returns, call.span)?,
            });
        }
        if let Some(capability) = self.capabilities.get(symbol) {
            let args = self.build_exprs_as_values(&call.args)?;
            if let Some(family) = capability.hash_family {
                return Ok(TypedExpr {
                    expr: hir::Expr::Hash(hir::HashExpr {
                        family,
                        inputs: capability.inputs.clone(),
                        args: args.into_iter().map(|(expr, _)| expr).collect(),
                        span: call.span,
                    }),
                    ty: Some(TYPE_BYTES32_ID),
                });
            }
            return Ok(TypedExpr {
                expr: hir::Expr::CallCapability(hir::CallCapabilityExpr {
                    capability: capability.id,
                    args: args.into_iter().map(|(expr, _)| expr).collect(),
                    outputs: capability.outputs.clone(),
                    span: call.span,
                }),
                ty: single_output_ty(&capability.outputs, call.span)?,
            });
        }
        Err(FrontendError::new(
            FrontendErrorKind::UndefinedSymbol,
            call.span,
            format!("unresolved call target {}", call.callee.as_string()),
        ))
    }

    fn build_table_read(&mut self, read: &ast::TableReadExpr) -> Result<TypedExpr, FrontendError> {
        let table_symbol = single_segment(&read.table, read.span)?;
        let table = self.tables.get(table_symbol).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                read.table.span,
                format!("unknown table {}", read.table.as_string()),
            )
        })?;
        let field = table.fields.get(&read.field).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                read.field_span,
                format!("unknown state field {}", read.field),
            )
        })?;
        let key = self.build_exprs_as_values(&read.key)?;
        Ok(TypedExpr {
            expr: hir::Expr::TableRead(hir::TableReadExpr {
                table: table.id,
                key: key.into_iter().map(|(expr, _)| expr).collect(),
                field: field.id,
                ty: field.ty,
                span: read.span,
            }),
            ty: Some(field.ty),
        })
    }

    fn build_eval_relation(
        &mut self,
        eval: &ast::EvalRelationExpr,
    ) -> Result<TypedExpr, FrontendError> {
        let relation_symbol = single_segment(&eval.relation, eval.span)?;
        let relation = self.relations.get(relation_symbol).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                eval.relation.span,
                format!("unknown relation {}", eval.relation.as_string()),
            )
        })?;
        let args = self.build_exprs_as_values(&eval.args)?;
        Ok(TypedExpr {
            expr: hir::Expr::EvalRelation(hir::EvalRelationExpr {
                relation: relation.id,
                args: args.into_iter().map(|(expr, _)| expr).collect(),
                outputs: relation.outputs.clone(),
                span: eval.span,
            }),
            ty: single_output_ty(&relation.outputs, eval.span)?,
        })
    }

    fn build_select_expr(&mut self, select: &ast::SelectExpr) -> Result<TypedExpr, FrontendError> {
        let cond = self.build_expr(&select.cond)?;
        let (cond_expr, _) = cond.require_value("select condition must be value-producing")?;
        let if_true = self.build_expr(&select.if_true)?;
        let if_false = self.build_expr(&select.if_false)?;
        let (if_true_expr, if_true_ty) =
            if_true.require_value("select branch must be value-producing")?;
        let (if_false_expr, _) = if_false.require_value("select branch must be value-producing")?;
        Ok(TypedExpr {
            expr: hir::Expr::Select(hir::SelectExpr {
                cond: Box::new(cond_expr),
                if_true: Box::new(if_true_expr),
                if_false: Box::new(if_false_expr),
                ty: if_true_ty,
                span: select.span,
            }),
            ty: Some(if_true_ty),
        })
    }
}
