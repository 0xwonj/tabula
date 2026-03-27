#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use super::consts::{ensure_type, single_segment};
#[allow(clippy::wildcard_imports)]
use super::*;
use crate::error::{FrontendError, FrontendErrorKind};

impl<'a> BodyBuildCx<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        top_level_names: &'a BTreeSet<String>,
        context_fields: &'a BTreeMap<String, BuiltContextFieldInfo>,
        tables: &'a BTreeMap<String, BuiltTableInfo>,
        consts: &'a BTreeMap<String, BuiltConstInfo>,
        relations: &'a BTreeMap<String, BuiltRelationInfo>,
        events: &'a BTreeMap<String, BuiltEventInfo>,
        callables: &'a BTreeMap<String, BuiltCallableInfo>,
        capabilities: &'a BTreeMap<String, hir::CapabilityDescriptor>,
        params: &'a [hir::ParamDecl],
        returns: &'a [hir::TypeRef],
    ) -> Self {
        Self {
            top_level_names,
            context_fields,
            tables,
            consts,
            relations,
            events,
            callables,
            capabilities,
            params,
            returns,
            bindings: BTreeMap::new(),
            next_binding_id: 0,
        }
    }

    pub(super) fn build_body(mut self, block: &ast::Block) -> Result<hir::Body, FrontendError> {
        Ok(hir::Body {
            region: self.build_region(block, RegionTermKind::Root)?,
        })
    }

    pub(super) fn build_region(
        &mut self,
        block: &ast::Block,
        kind: RegionTermKind,
    ) -> Result<hir::Region, FrontendError> {
        let mut statements = Vec::new();
        for statement in &block.statements {
            statements.push(self.build_stmt(statement)?);
        }
        let terminator = match (&block.return_value, block.return_span) {
            (Some(expr), Some(span)) => {
                let typed = self.build_expr(expr)?;
                let (expr, ty) =
                    typed.require_value("return requires a value-producing expression")?;
                if kind == RegionTermKind::Root {
                    if self.returns.len() != 1 {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            span,
                            "return expression is only valid for single-result functions",
                        ));
                    }
                    ensure_type(ty, self.returns[0], span, "return type mismatch")?;
                }
                hir::Terminator::Return {
                    values: vec![expr],
                    span,
                }
            }
            (None, Some(span)) => {
                if kind == RegionTermKind::Root && !self.returns.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        span,
                        "missing return value for non-unit function",
                    ));
                }
                hir::Terminator::Return {
                    values: Vec::new(),
                    span,
                }
            }
            (None, None) => {
                if kind == RegionTermKind::Root && !self.returns.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        block.span,
                        "non-unit function requires explicit return",
                    ));
                }
                match kind {
                    RegionTermKind::Root => hir::Terminator::Return {
                        values: Vec::new(),
                        span: block.span,
                    },
                    RegionTermKind::Nested => hir::Terminator::Yield {
                        values: Vec::new(),
                        span: block.span,
                    },
                }
            }
            (Some(_), None) => unreachable!(),
        };
        Ok(hir::Region {
            statements,
            terminator,
            span: block.span,
        })
    }

    pub(super) fn build_nested_region(
        &mut self,
        block: &ast::Block,
    ) -> Result<hir::Region, FrontendError> {
        let saved_bindings = self.bindings.clone();
        let region = self.build_region(block, RegionTermKind::Nested);
        self.bindings = saved_bindings;
        region
    }

    pub(super) fn build_empty_nested_region(span: Span) -> hir::Region {
        hir::Region {
            statements: Vec::new(),
            terminator: hir::Terminator::Yield {
                values: Vec::new(),
                span,
            },
            span,
        }
    }

    pub(super) fn build_stmt(&mut self, statement: &ast::Stmt) -> Result<hir::Stmt, FrontendError> {
        match statement {
            ast::Stmt::Let(stmt) => self.build_let(stmt).map(hir::Stmt::Let),
            ast::Stmt::StateAssign(stmt) => {
                self.build_state_assign(stmt).map(hir::Stmt::StateAssign)
            }
            ast::Stmt::Assert(stmt) => self.build_assert(stmt).map(hir::Stmt::Assert),
            ast::Stmt::Emit(stmt) => self.build_emit(stmt).map(hir::Stmt::Emit),
            ast::Stmt::If(stmt) => self.build_if(stmt).map(hir::Stmt::If),
            ast::Stmt::Match(stmt) => self.build_match(stmt).map(hir::Stmt::Match),
            ast::Stmt::Expr(stmt) => self.build_expr_stmt(stmt).map(hir::Stmt::Expr),
        }
    }

    pub(super) fn build_let(
        &mut self,
        statement: &ast::LetStmt,
    ) -> Result<hir::LetStmt, FrontendError> {
        let ast::Pattern::Name(symbol, span) = &statement.pattern else {
            return Err(FrontendError::new(
                FrontendErrorKind::UnsupportedFeature,
                statement.span,
                "tuple patterns are intentionally deferred to a later phase",
            ));
        };
        self.ensure_bindable_name(symbol, *span)?;
        let typed = self.build_expr(&statement.value)?;
        let (value, ty) = typed.require_value("let requires a value-producing expression")?;
        let binding = hir::BindingDecl {
            id: hir::BindingId(self.next_binding_id),
            symbol: symbol.clone(),
            ty,
            span: *span,
        };
        self.next_binding_id += 1;
        self.bindings
            .insert(symbol.clone(), BindingInfo { id: binding.id, ty });
        Ok(hir::LetStmt {
            binding,
            value,
            span: statement.span,
        })
    }

    pub(super) fn build_state_assign(
        &mut self,
        statement: &ast::StateAssignStmt,
    ) -> Result<hir::StateAssignStmt, FrontendError> {
        let table_symbol = single_segment(&statement.table, statement.span)?;
        let table = self.tables.get(table_symbol).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                statement.table.span,
                format!("unknown table {}", statement.table.as_string()),
            )
        })?;
        let field = table.fields.get(&statement.field).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                statement.field_span,
                format!("unknown state field {}", statement.field),
            )
        })?;
        let key = self.build_exprs_as_values(&statement.key)?;
        let typed = self.build_expr(&statement.value)?;
        let (value, _) = typed.require_value("state assignment requires a value")?;
        Ok(hir::StateAssignStmt {
            target: hir::StatePlace {
                table: table.id,
                key: key.into_iter().map(|(expr, _)| expr).collect(),
                field: field.id,
                span: statement.span,
            },
            value,
            span: statement.span,
        })
    }

    pub(super) fn build_assert(
        &mut self,
        statement: &ast::AssertStmt,
    ) -> Result<hir::AssertStmt, FrontendError> {
        match statement {
            ast::AssertStmt::Expr { expr, span } => {
                let typed = self.build_expr(expr)?;
                let (expr, _) =
                    typed.require_value("assert requires a value-producing expression")?;
                Ok(hir::AssertStmt::Expr { expr, span: *span })
            }
            ast::AssertStmt::Relation {
                relation,
                args,
                span,
            } => {
                let relation_symbol = single_segment(relation, *span)?;
                let relation_info = self.relations.get(relation_symbol).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        relation.span,
                        format!("unknown relation {}", relation.as_string()),
                    )
                })?;
                let args = self.build_exprs_as_values(args)?;
                Ok(hir::AssertStmt::Relation {
                    relation: relation_info.id,
                    args: args.into_iter().map(|(expr, _)| expr).collect(),
                    span: *span,
                })
            }
        }
    }

    pub(super) fn build_expr_stmt(
        &mut self,
        statement: &ast::ExprStmt,
    ) -> Result<hir::ExprStmt, FrontendError> {
        let typed = self.build_expr(&statement.expr)?;
        Ok(hir::ExprStmt {
            expr: typed.expr,
            span: statement.span,
        })
    }

    pub(super) fn build_emit(
        &mut self,
        statement: &ast::EmitStmt,
    ) -> Result<hir::EmitStmt, FrontendError> {
        let event_symbol = single_segment(&statement.event, statement.span)?;
        let event = self.events.get(event_symbol).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                statement.event.span,
                format!("unknown event {}", statement.event.as_string()),
            )
        })?;
        let args = self.build_exprs_as_values(&statement.args)?;
        Ok(hir::EmitStmt {
            event: event.id,
            args: args.into_iter().map(|(expr, _)| expr).collect(),
            span: statement.span,
        })
    }

    pub(super) fn build_if(
        &mut self,
        statement: &ast::IfStmt,
    ) -> Result<hir::IfStmt, FrontendError> {
        let typed = self.build_expr(&statement.cond)?;
        let (cond, _) = typed.require_value("if condition must be value-producing")?;
        let then_region = self.build_nested_region(&statement.then_block)?;
        let else_region = match &statement.else_block {
            Some(block) => self.build_nested_region(block)?,
            None => Self::build_empty_nested_region(statement.span),
        };
        Ok(hir::IfStmt {
            cond,
            then_region,
            else_region,
            span: statement.span,
        })
    }

    pub(super) fn build_match(
        &mut self,
        statement: &ast::MatchStmt,
    ) -> Result<hir::MatchStmt, FrontendError> {
        let typed = self.build_expr(&statement.scrutinee)?;
        let (scrutinee, scrutinee_ty) =
            typed.require_value("match scrutinee must be value-producing")?;
        let mut arms = Vec::new();
        let mut default = None;
        let mut seen_default = false;
        for (index, arm) in statement.arms.iter().enumerate() {
            if seen_default {
                return Err(FrontendError::new(
                    FrontendErrorKind::InvalidProgram,
                    arm.span,
                    "wildcard match arm must be last",
                ));
            }
            match &arm.pattern {
                ast::MatchPattern::Literal(literal) => {
                    let region = self.build_nested_region(&arm.block)?;
                    arms.push(hir::MatchArm {
                        pattern: hir::MatchPattern::Literal(super::consts::build_literal_value(
                            &literal.kind,
                            Some(scrutinee_ty),
                            literal.span,
                        )?),
                        region,
                    });
                }
                ast::MatchPattern::Wildcard(_) => {
                    if index + 1 != statement.arms.len() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::InvalidProgram,
                            arm.span,
                            "wildcard match arm must be last",
                        ));
                    }
                    default = Some(self.build_nested_region(&arm.block)?);
                    seen_default = true;
                }
            }
        }
        Ok(hir::MatchStmt {
            scrutinee,
            arms,
            default,
            span: statement.span,
        })
    }

    pub(super) fn build_exprs_as_values(
        &mut self,
        exprs: &[ast::Expr],
    ) -> Result<Vec<(hir::Expr, hir::TypeRef)>, FrontendError> {
        exprs
            .iter()
            .map(|expr| {
                self.build_expr(expr)?
                    .require_value("expected value expression")
            })
            .collect()
    }

    pub(super) fn ensure_bindable_name(
        &self,
        symbol: &str,
        span: Span,
    ) -> Result<(), FrontendError> {
        if self.top_level_names.contains(symbol) {
            return Err(FrontendError::new(
                FrontendErrorKind::DuplicateSymbol,
                span,
                format!("local binding {symbol} may not shadow top-level symbol"),
            ));
        }
        if self.context_fields.contains_key(symbol) {
            return Err(FrontendError::new(
                FrontendErrorKind::DuplicateSymbol,
                span,
                format!("local binding {symbol} may not shadow context field"),
            ));
        }
        if self.bindings.contains_key(symbol)
            || self.params.iter().any(|param| param.symbol == symbol)
        {
            return Err(FrontendError::new(
                FrontendErrorKind::DuplicateSymbol,
                span,
                format!("duplicate local binding {symbol}"),
            ));
        }
        Ok(())
    }
}
