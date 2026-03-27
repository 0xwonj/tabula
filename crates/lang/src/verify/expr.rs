#![allow(clippy::wildcard_imports)]
#![allow(missing_docs)]

use super::*;
use tabula_profile::{is_bool_type, is_bytes32_type, is_i64_type};

impl<'a> VerifyCx<'a> {
    pub(super) fn verify_expr(
        &self,
        expr: &Expr,
        locals: &LocalEnv,
    ) -> Result<Option<TypeRef>, FrontendError> {
        match expr {
            Expr::Literal(expr) => Ok(Some(expr.value.type_id())),
            Expr::Local(expr) => match expr.local {
                LocalRef::Param(param) => {
                    locals.params.get(&param).copied().map(Some).ok_or_else(|| {
                        FrontendError::new(
                            FrontendErrorKind::UndefinedSymbol,
                            expr.span,
                            format!("unknown param id {}", param.0),
                        )
                    })
                }
                LocalRef::Binding(binding) => locals
                    .bindings
                    .get(&binding)
                    .copied()
                    .map(Some)
                    .ok_or_else(|| {
                        FrontendError::new(
                            FrontendErrorKind::UndefinedSymbol,
                            expr.span,
                            format!("unknown binding id {}", binding.0),
                        )
                    }),
            },
            Expr::Const(expr) => self
                .consts
                .get(&expr.const_id)
                .map(|decl| Some(decl.ty))
                .ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown const id {}", expr.const_id.0),
                    )
                }),
            Expr::Context(expr) => self
                .context_fields
                .get(&expr.field)
                .map(|field| Some(field.ty))
                .ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown context field id {}", expr.field.0),
                    )
                }),
            Expr::TableRead(expr) => {
                let table = self.tables.get(&expr.table).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown table id {}", expr.table.0),
                    )
                })?;
                let field = self
                    .table_fields
                    .get(&(expr.table, expr.field))
                    .copied()
                    .ok_or_else(|| {
                        FrontendError::new(
                            FrontendErrorKind::UndefinedSymbol,
                            expr.span,
                            format!(
                                "unknown field id {} on table {}",
                                expr.field.0, table.symbol
                            ),
                        )
                    })?;
                if expr.key.len() != table.keys.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "table key arity mismatch",
                    ));
                }
                for (key_expr, key) in expr.key.iter().zip(&table.keys) {
                    ensure_type(
                        self.require_value_type(
                            key_expr,
                            locals,
                            "table key requires value-producing expression",
                        )?,
                        key.ty,
                        expr.span,
                        "table key type mismatch",
                    )?;
                }
                ensure_type(expr.ty, field.ty, expr.span, "table read type mismatch")?;
                Ok(Some(field.ty))
            }
            Expr::Unary(expr) => {
                let operand_ty = self.require_value_type(
                    &expr.expr,
                    locals,
                    "unary operator requires value operand",
                )?;
                match expr.op {
                    UnaryOp::Not => {
                        ensure_type(
                            operand_ty,
                            tabula_profile::TYPE_BOOL_ID,
                            expr.span,
                            "logical not requires bool operand",
                        )?;
                        if !is_bool_type(expr.ty) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                expr.span,
                                "logical not result type mismatch",
                            ));
                        }
                    }
                    UnaryOp::Neg => {
                        ensure_type(
                            operand_ty,
                            tabula_profile::TYPE_I64_ID,
                            expr.span,
                            "unary minus requires i64 operand",
                        )?;
                        if !is_i64_type(expr.ty) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                expr.span,
                                "unary minus result type mismatch",
                            ));
                        }
                    }
                }
                Ok(Some(expr.ty))
            }
            Expr::Binary(expr) => {
                let lhs_ty = self.require_value_type(
                    &expr.lhs,
                    locals,
                    "binary operator requires value operands",
                )?;
                let rhs_ty = self.require_value_type(
                    &expr.rhs,
                    locals,
                    "binary operator requires value operands",
                )?;
                ensure_type(lhs_ty, rhs_ty, expr.span, "binary operand type mismatch")?;
                match expr.op {
                    BinaryOp::Eq | BinaryOp::Ne => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Equality,
                            expr.span,
                            "type does not support equality",
                        )?;
                        if !is_bool_type(expr.ty) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                expr.span,
                                "comparison/logical result type mismatch",
                            ));
                        }
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Ordering,
                            expr.span,
                            "type does not support ordering",
                        )?;
                        if !is_bool_type(expr.ty) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                expr.span,
                                "comparison/logical result type mismatch",
                            ));
                        }
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        ensure_type(
                            lhs_ty,
                            tabula_profile::TYPE_BOOL_ID,
                            expr.span,
                            "logical operators require bool operands",
                        )?;
                        if !is_bool_type(expr.ty) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                expr.span,
                                "comparison/logical result type mismatch",
                            ));
                        }
                    }
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Arithmetic,
                            expr.span,
                            "type does not support arithmetic",
                        )?;
                        ensure_type(
                            expr.ty,
                            lhs_ty,
                            expr.span,
                            "arithmetic result type mismatch",
                        )?;
                    }
                }
                Ok(Some(expr.ty))
            }
            Expr::CallFunction(expr) => {
                let callable = self.callables.get(&expr.callee).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown callable id {}", expr.callee.0),
                    )
                })?;
                if callable.kind != CallableKind::Function {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "bare calls may only target internal fn declarations",
                    ));
                }
                if expr.args.len() != callable.params.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "function call argument mismatch",
                    ));
                }
                for (arg, param) in expr.args.iter().zip(&callable.params) {
                    ensure_type(
                        self.require_value_type(
                            arg,
                            locals,
                            "function call arguments must be value-producing",
                        )?,
                        param.ty,
                        expr.span,
                        "function call argument mismatch",
                    )?;
                }
                if expr.returns != callable.returns {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "function call return signature mismatch",
                    ));
                }
                Ok(single_output(expr.returns.as_slice()))
            }
            Expr::CallCapability(expr) => {
                let capability = self.capabilities.get(&expr.capability).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown capability id {}", expr.capability.0),
                    )
                })?;
                if capability.hash_family.is_some() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "blessed hash capabilities must lower to Hash expressions",
                    ));
                }
                if expr.args.len() != capability.inputs.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "capability call argument mismatch",
                    ));
                }
                for (arg, expected) in expr.args.iter().zip(&capability.inputs) {
                    ensure_type(
                        self.require_value_type(
                            arg,
                            locals,
                            "capability call arguments must be value-producing",
                        )?,
                        *expected,
                        expr.span,
                        "capability call argument mismatch",
                    )?;
                }
                if expr.outputs != capability.outputs {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "capability call output signature mismatch",
                    ));
                }
                Ok(single_output(expr.outputs.as_slice()))
            }
            Expr::Hash(expr) => {
                if self
                    .capability_signatures
                    .iter()
                    .find(|capability| {
                        capability.hash_family == Some(expr.family)
                            && capability.inputs == expr.inputs
                            && capability.outputs.len() == 1
                            && is_bytes32_type(capability.outputs[0])
                    })
                    .is_none()
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        "hash expression does not match any imported blessed builtin capability",
                    ));
                }
                if expr.args.len() != expr.inputs.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "hash argument mismatch",
                    ));
                }
                for (arg, expected) in expr.args.iter().zip(&expr.inputs) {
                    ensure_type(
                        self.require_value_type(
                            arg,
                            locals,
                            "hash arguments must be value-producing",
                        )?,
                        *expected,
                        expr.span,
                        "hash argument mismatch",
                    )?;
                }
                Ok(Some(tabula_profile::TYPE_BYTES32_ID))
            }
            Expr::EvalRelation(expr) => {
                let relation = self.relations.get(&expr.relation).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        expr.span,
                        format!("unknown relation id {}", expr.relation.0),
                    )
                })?;
                if relation.results.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "eval relation requires functional relation with outputs",
                    ));
                }
                if expr.args.len() != relation.params.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "relation argument mismatch",
                    ));
                }
                for (arg, param) in expr.args.iter().zip(&relation.params) {
                    ensure_type(
                        self.require_value_type(
                            arg,
                            locals,
                            "relation arguments must be value-producing",
                        )?,
                        param.ty,
                        expr.span,
                        "relation argument mismatch",
                    )?;
                }
                let relation_outputs = relation
                    .results
                    .iter()
                    .map(|result| result.ty)
                    .collect::<Vec<_>>();
                if expr.outputs != relation_outputs {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        expr.span,
                        "eval relation output signature mismatch",
                    ));
                }
                Ok(single_output(expr.outputs.as_slice()))
            }
            Expr::Select(expr) => {
                ensure_type(
                    self.require_value_type(
                        &expr.cond,
                        locals,
                        "select condition must be value-producing",
                    )?,
                    tabula_profile::TYPE_BOOL_ID,
                    expr.span,
                    "select condition must be bool",
                )?;
                let if_true = self.require_value_type(
                    &expr.if_true,
                    locals,
                    "select branch must be value-producing",
                )?;
                let if_false = self.require_value_type(
                    &expr.if_false,
                    locals,
                    "select branch must be value-producing",
                )?;
                ensure_type(if_true, if_false, expr.span, "select branch type mismatch")?;
                ensure_type(expr.ty, if_true, expr.span, "select result type mismatch")?;
                Ok(Some(expr.ty))
            }
        }
    }

    pub(super) fn require_value_type(
        &self,
        expr: &Expr,
        locals: &LocalEnv,
        message: &'static str,
    ) -> Result<TypeRef, FrontendError> {
        self.verify_expr(expr, locals)?.ok_or_else(|| {
            FrontendError::new(FrontendErrorKind::TypeMismatch, expr.span(), message)
        })
    }

    pub(super) fn verify_const_expr(&self, expr: &ConstExpr) -> Result<TypeRef, FrontendError> {
        match expr {
            ConstExpr::Literal(value) => Ok(value.type_id()),
            ConstExpr::Unary { op, expr } => {
                let operand_ty = self.verify_const_expr(expr)?;
                match op {
                    UnaryOp::Not => {
                        ensure_type(
                            operand_ty,
                            tabula_profile::TYPE_BOOL_ID,
                            self.program.span,
                            "const logical not requires bool operand",
                        )?;
                        Ok(tabula_profile::TYPE_BOOL_ID)
                    }
                    UnaryOp::Neg => {
                        ensure_type(
                            operand_ty,
                            tabula_profile::TYPE_I64_ID,
                            self.program.span,
                            "const unary minus requires i64 operand",
                        )?;
                        Ok(tabula_profile::TYPE_I64_ID)
                    }
                }
            }
            ConstExpr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.verify_const_expr(lhs)?;
                let rhs_ty = self.verify_const_expr(rhs)?;
                ensure_type(
                    lhs_ty,
                    rhs_ty,
                    self.program.span,
                    "const binary operand type mismatch",
                )?;
                Ok(match op {
                    BinaryOp::Eq | BinaryOp::Ne => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Equality,
                            self.program.span,
                            "type does not support equality",
                        )?;
                        tabula_profile::TYPE_BOOL_ID
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Ordering,
                            self.program.span,
                            "type does not support ordering",
                        )?;
                        tabula_profile::TYPE_BOOL_ID
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        ensure_type(
                            lhs_ty,
                            tabula_profile::TYPE_BOOL_ID,
                            self.program.span,
                            "const logical operators require bool operands",
                        )?;
                        tabula_profile::TYPE_BOOL_ID
                    }
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod => {
                        self.prelude.require_type_capability(
                            lhs_ty,
                            TypeCapabilityKind::Arithmetic,
                            self.program.span,
                            "type does not support arithmetic",
                        )?;
                        lhs_ty
                    }
                })
            }
        }
    }
}
