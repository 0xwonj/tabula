#![allow(missing_docs)]

#[allow(clippy::wildcard_imports)]
use super::*;

impl<'a> VerifyCx<'a> {
    pub(super) fn verify_callable(&self, callable: &CallableDecl) -> Result<(), FrontendError> {
        if callable.kind == CallableKind::Tx && !callable.returns.is_empty() {
            return Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                callable.span,
                format!("tx {} must not declare return values", callable.symbol),
            ));
        }
        if callable.kind == CallableKind::Query && callable.returns.len() != 1 {
            return Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                callable.span,
                format!(
                    "query {} must declare exactly one return type",
                    callable.symbol
                ),
            ));
        }
        let mut param_ids = BTreeSet::new();
        let mut param_symbols = BTreeSet::new();
        let mut locals = LocalEnv::default();
        for param in &callable.params {
            if !param_ids.insert(param.id) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    param.span,
                    format!("duplicate param id {}", param.id.0),
                ));
            }
            if !param_symbols.insert(param.symbol.clone()) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    param.span,
                    format!("duplicate param {}", param.symbol),
                ));
            }
            if self.top_level_symbols.contains(&param.symbol) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    param.span,
                    format!("param {} may not shadow top-level symbol", param.symbol),
                ));
            }
            if self
                .context_fields
                .values()
                .any(|field| field.symbol == param.symbol)
            {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    param.span,
                    format!("param {} may not shadow context field", param.symbol),
                ));
            }
            locals.params.insert(param.id, param.ty);
            locals.param_symbols.insert(param.symbol.clone());
        }
        self.verify_region(
            &callable.body.region,
            callable,
            callable.kind,
            &mut locals,
            RegionKind::Root,
        )
    }

    pub(super) fn verify_region(
        &self,
        region: &Region,
        callable: &CallableDecl,
        callable_kind: CallableKind,
        locals: &mut LocalEnv,
        kind: RegionKind,
    ) -> Result<(), FrontendError> {
        for statement in &region.statements {
            self.verify_stmt(statement, callable_kind, callable, locals)?;
        }
        match (&region.terminator, kind) {
            (Terminator::Return { values, span }, RegionKind::Root) => {
                if callable.kind == CallableKind::Tx && !values.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        *span,
                        format!("tx {} must use unit return", callable.symbol),
                    ));
                }
                if values.len() != callable.returns.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        *span,
                        format!(
                            "callable {} return arity mismatch: expected {}, got {}",
                            callable.symbol,
                            callable.returns.len(),
                            values.len()
                        ),
                    ));
                }
                for (value, expected) in values.iter().zip(&callable.returns) {
                    ensure_type(
                        self.require_value_type(
                            value,
                            locals,
                            "return requires value-producing expression",
                        )?,
                        *expected,
                        *span,
                        "return type mismatch",
                    )?;
                }
                Ok(())
            }
            (Terminator::Yield { values, span }, RegionKind::Nested) => {
                if !values.is_empty() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        *span,
                        "nested control regions must yield zero values in exact V3",
                    ));
                }
                Ok(())
            }
            (Terminator::Yield { span, .. }, RegionKind::Root) => Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                *span,
                "root callable region must terminate with return",
            )),
            (Terminator::Return { span, .. }, RegionKind::Nested) => Err(FrontendError::new(
                FrontendErrorKind::InvalidProgram,
                *span,
                "return is not allowed inside nested if/match branches in exact V3",
            )),
        }
    }

    pub(super) fn verify_stmt(
        &self,
        stmt: &Stmt,
        callable_kind: CallableKind,
        callable: &CallableDecl,
        locals: &mut LocalEnv,
    ) -> Result<(), FrontendError> {
        match stmt {
            Stmt::Let(let_stmt) => {
                if self.top_level_symbols.contains(&let_stmt.binding.symbol) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        let_stmt.binding.span,
                        format!(
                            "local binding {} may not shadow top-level symbol",
                            let_stmt.binding.symbol
                        ),
                    ));
                }
                if self
                    .context_fields
                    .values()
                    .any(|field| field.symbol == let_stmt.binding.symbol)
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        let_stmt.binding.span,
                        format!(
                            "local binding {} may not shadow context field",
                            let_stmt.binding.symbol
                        ),
                    ));
                }
                if locals.param_symbols.contains(&let_stmt.binding.symbol)
                    || locals.binding_symbols.contains(&let_stmt.binding.symbol)
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        let_stmt.binding.span,
                        format!("duplicate local binding {}", let_stmt.binding.symbol),
                    ));
                }
                let actual = self.require_value_type(
                    &let_stmt.value,
                    locals,
                    "let requires a value-producing expression",
                )?;
                ensure_type(
                    actual,
                    let_stmt.binding.ty,
                    let_stmt.span,
                    "let binding type mismatch",
                )?;
                if locals
                    .bindings
                    .insert(let_stmt.binding.id, let_stmt.binding.ty)
                    .is_some()
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        let_stmt.binding.span,
                        format!("duplicate binding id {}", let_stmt.binding.id.0),
                    ));
                }
                locals
                    .binding_symbols
                    .insert(let_stmt.binding.symbol.clone());
            }
            Stmt::StateAssign(assign) => {
                if callable_kind == CallableKind::Query {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        assign.span,
                        "query bodies may not write state directly",
                    ));
                }
                let table = self.tables.get(&assign.target.table).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        assign.target.span,
                        format!("unknown table id {}", assign.target.table.0),
                    )
                })?;
                let field = self
                    .table_fields
                    .get(&(assign.target.table, assign.target.field))
                    .copied()
                    .ok_or_else(|| {
                        FrontendError::new(
                            FrontendErrorKind::UndefinedSymbol,
                            assign.target.span,
                            format!(
                                "unknown field id {} on table {}",
                                assign.target.field.0, table.symbol
                            ),
                        )
                    })?;
                if assign.target.key.len() != table.keys.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        assign.target.span,
                        "state key arity mismatch",
                    ));
                }
                for (expr, key) in assign.target.key.iter().zip(&table.keys) {
                    ensure_type(
                        self.require_value_type(
                            expr,
                            locals,
                            "state key requires value-producing expression",
                        )?,
                        key.ty,
                        assign.target.span,
                        "state key type mismatch",
                    )?;
                }
                ensure_type(
                    self.require_value_type(
                        &assign.value,
                        locals,
                        "state assignment requires value-producing expression",
                    )?,
                    field.ty,
                    assign.span,
                    "state assignment type mismatch",
                )?;
            }
            Stmt::Assert(assert_stmt) => match assert_stmt {
                AssertStmt::Expr { expr, span } => {
                    ensure_type(
                        self.require_value_type(
                            expr,
                            locals,
                            "assert requires value-producing expression",
                        )?,
                        tabula_profile::TYPE_BOOL_ID,
                        *span,
                        "assert requires bool condition",
                    )?;
                }
                AssertStmt::Relation {
                    relation,
                    args,
                    span,
                } => {
                    let relation = self.relations.get(relation).ok_or_else(|| {
                        FrontendError::new(
                            FrontendErrorKind::UndefinedSymbol,
                            *span,
                            format!("unknown relation id {}", relation.0),
                        )
                    })?;
                    if !relation.results.is_empty() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            *span,
                            "assert relation requires output-free relation",
                        ));
                    }
                    if args.len() != relation.params.len() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            *span,
                            "relation argument arity mismatch",
                        ));
                    }
                    for (arg, param) in args.iter().zip(&relation.params) {
                        ensure_type(
                            self.require_value_type(
                                arg,
                                locals,
                                "relation argument requires value-producing expression",
                            )?,
                            param.ty,
                            *span,
                            "relation argument type mismatch",
                        )?;
                    }
                }
            },
            Stmt::Emit(emit_stmt) => {
                if callable_kind == CallableKind::Query {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        emit_stmt.span,
                        "query bodies may not emit events directly",
                    ));
                }
                let event = self.events.get(&emit_stmt.event).ok_or_else(|| {
                    FrontendError::new(
                        FrontendErrorKind::UndefinedSymbol,
                        emit_stmt.span,
                        format!("unknown event id {}", emit_stmt.event.0),
                    )
                })?;
                if emit_stmt.args.len() != event.fields.len() {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        emit_stmt.span,
                        "event argument arity mismatch",
                    ));
                }
                for (arg, field) in emit_stmt.args.iter().zip(&event.fields) {
                    ensure_type(
                        self.require_value_type(
                            arg,
                            locals,
                            "event arguments must be value-producing",
                        )?,
                        field.ty,
                        emit_stmt.span,
                        "event argument type mismatch",
                    )?;
                }
            }
            Stmt::If(if_stmt) => {
                ensure_type(
                    self.require_value_type(
                        &if_stmt.cond,
                        locals,
                        "if condition must be value-producing",
                    )?,
                    tabula_profile::TYPE_BOOL_ID,
                    if_stmt.span,
                    "if condition must be bool",
                )?;
                let mut then_locals = locals.clone();
                self.verify_region(
                    &if_stmt.then_region,
                    callable,
                    callable_kind,
                    &mut then_locals,
                    RegionKind::Nested,
                )?;
                let mut else_locals = locals.clone();
                self.verify_region(
                    &if_stmt.else_region,
                    callable,
                    callable_kind,
                    &mut else_locals,
                    RegionKind::Nested,
                )?;
            }
            Stmt::Match(match_stmt) => {
                let scrutinee_ty = self.require_value_type(
                    &match_stmt.scrutinee,
                    locals,
                    "match scrutinee must be value-producing",
                )?;
                let mut seen_literals = Vec::new();
                for arm in &match_stmt.arms {
                    let MatchPattern::Literal(value) = &arm.pattern;
                    ensure_type(
                        value.type_id(),
                        scrutinee_ty,
                        match_stmt.span,
                        "match literal pattern type mismatch",
                    )?;
                    if seen_literals.iter().any(|seen| seen == value) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::InvalidProgram,
                            match_stmt.span,
                            "duplicate match literal pattern",
                        ));
                    }
                    seen_literals.push(value.clone());
                    let mut arm_locals = locals.clone();
                    self.verify_region(
                        &arm.region,
                        callable,
                        callable_kind,
                        &mut arm_locals,
                        RegionKind::Nested,
                    )?;
                }
                if let Some(default) = &match_stmt.default {
                    let mut default_locals = locals.clone();
                    self.verify_region(
                        default,
                        callable,
                        callable_kind,
                        &mut default_locals,
                        RegionKind::Nested,
                    )?;
                }
            }
            Stmt::Expr(expr_stmt) => {
                if !matches!(
                    expr_stmt.expr,
                    Expr::CallFunction(_) | Expr::CallCapability(_)
                ) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::InvalidProgram,
                        expr_stmt.span,
                        "expression statements must be function or capability calls in V2",
                    ));
                }
                let _ = self.verify_expr(&expr_stmt.expr, locals)?;
            }
        }
        Ok(())
    }
}
