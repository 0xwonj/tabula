#![allow(clippy::wildcard_imports)]
#![allow(missing_docs)]

use crate::ast::*;
use crate::error::{FrontendError, FrontendErrorKind};
use crate::span::Span;
use crate::syntax::features::{DeferredSyntaxFeature, deferred_feature_error};
use crate::syntax::lexer;
use crate::syntax::token::Token;

pub fn parse_program(source: &str) -> Result<Program, FrontendError> {
    let tokens = lexer::lex(source)?;
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_program(&mut self) -> Result<Program, FrontendError> {
        let mut uses = Vec::new();
        while self.at(&Token::Use) {
            uses.push(self.parse_use_decl()?);
        }
        let program_start = self.expect(&Token::Program)?.1;
        let (symbol, symbol_span) = self.expect_ident()?;
        let mut decls = Vec::new();
        while !self.at(&Token::Eof) {
            decls.push(self.parse_top_decl()?);
        }
        let span = program_start.merge(symbol_span).merge(self.current_span());
        Ok(Program {
            symbol,
            uses,
            decls,
            span,
        })
    }

    fn parse_use_decl(&mut self) -> Result<UseDecl, FrontendError> {
        let start = self.expect(&Token::Use)?.1;
        self.expect(&Token::Capability)?;
        let path = self.parse_path()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(UseDecl {
            path,
            span: start.merge(end),
        })
    }

    fn parse_top_decl(&mut self) -> Result<TopDecl, FrontendError> {
        match self.current_token() {
            Token::Context => Ok(TopDecl::Context(self.parse_context_decl()?)),
            Token::State => Ok(TopDecl::State(self.parse_state_decl()?)),
            Token::Const => Ok(TopDecl::Const(self.parse_const_decl()?)),
            Token::Relation => Ok(TopDecl::Relation(self.parse_relation_decl()?)),
            Token::Event => Ok(TopDecl::Event(self.parse_event_decl()?)),
            Token::Fn | Token::Tx | Token::Query => {
                Ok(TopDecl::Callable(self.parse_callable_decl()?))
            }
            Token::Requires => Err(self.deferred_feature(DeferredSyntaxFeature::Requires)),
            Token::Ensures => Err(self.deferred_feature(DeferredSyntaxFeature::Ensures)),
            Token::Emit | Token::If | Token::Match => Err(self.error_here(
                FrontendErrorKind::UnexpectedToken,
                "expected top-level declaration",
            )),
            Token::For => Err(self.deferred_feature(DeferredSyntaxFeature::ForLoop)),
            Token::Predicate => Err(self.deferred_feature(DeferredSyntaxFeature::Predicate)),
            Token::Invariant => Err(self.deferred_feature(DeferredSyntaxFeature::Invariant)),
            _ => Err(self.error_here(
                FrontendErrorKind::UnexpectedToken,
                "expected top-level declaration",
            )),
        }
    }

    fn parse_context_decl(&mut self) -> Result<ContextDecl, FrontendError> {
        let start = self.expect(&Token::Context)?.1;
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !self.at(&Token::RBrace) {
            fields.push(self.parse_context_field_decl()?);
        }
        let end = self.expect(&Token::RBrace)?.1;
        Ok(ContextDecl {
            fields,
            span: start.merge(end),
        })
    }

    fn parse_context_field_decl(&mut self) -> Result<ContextFieldDecl, FrontendError> {
        let (symbol, start) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type_expr()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(ContextFieldDecl {
            symbol,
            ty,
            span: start.merge(end),
        })
    }

    fn parse_state_decl(&mut self) -> Result<StateDecl, FrontendError> {
        let start = self.expect(&Token::State)?.1;
        self.expect(&Token::LBrace)?;
        let mut tables = Vec::new();
        while !self.at(&Token::RBrace) {
            tables.push(self.parse_table_decl()?);
        }
        let end = self.expect(&Token::RBrace)?.1;
        Ok(StateDecl {
            tables,
            span: start.merge(end),
        })
    }

    fn parse_table_decl(&mut self) -> Result<TableDecl, FrontendError> {
        let start = self.expect(&Token::Table)?.1;
        let (symbol, _) = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        self.expect(&Token::Key)?;
        let mut keys = vec![self.parse_param_decl()?];
        while self.at(&Token::Comma) {
            self.bump();
            keys.push(self.parse_param_decl()?);
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !self.at(&Token::RBrace) {
            fields.push(self.parse_state_field_decl()?);
        }
        let end = self.expect(&Token::RBrace)?.1;
        Ok(TableDecl {
            symbol,
            keys,
            fields,
            span: start.merge(end),
        })
    }

    fn parse_state_field_decl(&mut self) -> Result<StateFieldDecl, FrontendError> {
        let (symbol, start) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type_expr()?;
        let scheme = if self.at(&Token::At) {
            self.bump();
            Some(self.parse_path()?)
        } else {
            None
        };
        let end = self.expect(&Token::Semi)?.1;
        Ok(StateFieldDecl {
            symbol,
            ty,
            scheme,
            span: start.merge(end),
        })
    }

    fn parse_const_decl(&mut self) -> Result<ConstDecl, FrontendError> {
        let start = self.expect(&Token::Const)?.1;
        let (symbol, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type_expr()?;
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(ConstDecl {
            symbol,
            ty,
            value,
            span: start.merge(end),
        })
    }

    fn parse_relation_decl(&mut self) -> Result<RelationDecl, FrontendError> {
        let start = self.expect(&Token::Relation)?.1;
        let (symbol, _) = self.expect_ident()?;
        let params = self.parse_param_list()?;
        let results = if self.at(&Token::Arrow) {
            self.bump();
            self.parse_result_list()?
        } else {
            Vec::new()
        };
        self.expect(&Token::Eq)?;
        let body = self.parse_relation_body()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(RelationDecl {
            symbol,
            params,
            results,
            body,
            span: start.merge(end),
        })
    }

    fn parse_event_decl(&mut self) -> Result<EventDecl, FrontendError> {
        let start = self.expect(&Token::Event)?.1;
        let (symbol, _) = self.expect_ident()?;
        let fields = self.parse_param_list()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(EventDecl {
            symbol,
            fields,
            span: start.merge(end),
        })
    }

    fn parse_relation_body(&mut self) -> Result<RelationBody, FrontendError> {
        match self.current_token() {
            Token::Enum => {
                let start = self.bump().1;
                self.expect(&Token::LBrace)?;
                let values = self.parse_delimited_exprs(&Token::RBrace)?;
                let end = self.expect(&Token::RBrace)?.1;
                Ok(RelationBody::Enum {
                    values,
                    span: start.merge(end),
                })
            }
            Token::Range => {
                let start = self.bump().1;
                self.expect(&Token::LParen)?;
                let first = self.parse_expr()?;
                self.expect(&Token::Comma)?;
                let second = self.parse_expr()?;
                let end = self.expect(&Token::RParen)?.1;
                Ok(RelationBody::Range {
                    start: Box::new(first),
                    end: Box::new(second),
                    span: start.merge(end),
                })
            }
            Token::Map => {
                let start = self.bump().1;
                self.expect(&Token::LBrace)?;
                let mut entries = Vec::new();
                while !self.at(&Token::RBrace) {
                    let item_start = self.current_span();
                    let inputs = self.parse_tuple_like()?;
                    self.expect(&Token::FatArrow)?;
                    let outputs = self.parse_tuple_like()?;
                    let item_end = self.current_span();
                    entries.push(RelationMapEntry {
                        inputs,
                        outputs,
                        span: item_start.merge(item_end),
                    });
                    if self.at(&Token::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let end = self.expect(&Token::RBrace)?.1;
                Ok(RelationBody::Map {
                    entries,
                    span: start.merge(end),
                })
            }
            Token::Set => {
                let start = self.bump().1;
                self.expect(&Token::LBrace)?;
                let mut tuples = Vec::new();
                while !self.at(&Token::RBrace) {
                    tuples.push(self.parse_tuple_like()?);
                    if self.at(&Token::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let end = self.expect(&Token::RBrace)?.1;
                Ok(RelationBody::Set {
                    tuples,
                    span: start.merge(end),
                })
            }
            Token::Extern => {
                let span = self.bump().1;
                Ok(RelationBody::Extern { span })
            }
            _ => Err(self.error_here(FrontendErrorKind::UnexpectedToken, "expected relation body")),
        }
    }

    fn parse_callable_decl(&mut self) -> Result<CallableDecl, FrontendError> {
        let (kind, start) = match self.current_token() {
            Token::Fn => (CallableKind::Function, self.bump().1),
            Token::Query => (CallableKind::Query, self.bump().1),
            Token::Tx => (CallableKind::Tx, self.bump().1),
            _ => {
                return Err(self.error_here(
                    FrontendErrorKind::UnexpectedToken,
                    "expected fn, query, or tx declaration",
                ));
            }
        };
        let (symbol, _) = self.expect_ident()?;
        let params = self.parse_param_list()?;
        let returns = match kind {
            CallableKind::Query => {
                self.expect(&Token::Arrow)?;
                vec![self.parse_type_expr()?]
            }
            _ => {
                if self.at(&Token::Arrow) {
                    self.bump();
                    vec![self.parse_type_expr()?]
                } else {
                    Vec::new()
                }
            }
        };
        if self.at(&Token::Requires) {
            return Err(self.deferred_feature(DeferredSyntaxFeature::Requires));
        }
        if self.at(&Token::Ensures) {
            return Err(self.deferred_feature(DeferredSyntaxFeature::Ensures));
        }
        let body = self.parse_block()?;
        Ok(CallableDecl {
            kind,
            symbol,
            params,
            returns,
            span: start.merge(body.span),
            body,
        })
    }

    fn parse_block(&mut self) -> Result<Block, FrontendError> {
        let start = self.expect(&Token::LBrace)?.1;
        let mut statements = Vec::new();
        let mut return_value = None;
        let mut return_span = None;
        while !self.at(&Token::RBrace) {
            if self.at(&Token::Return) {
                let start_return = self.bump().1;
                return_value = if self.at(&Token::Semi) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                let end = self.expect(&Token::Semi)?.1;
                return_span = Some(start_return.merge(end));
                if !self.at(&Token::RBrace) {
                    return Err(self.error_here(
                        FrontendErrorKind::InvalidProgram,
                        "return must terminate the current rewritten block",
                    ));
                }
                break;
            }
            statements.push(self.parse_stmt()?);
        }
        let end = self.expect(&Token::RBrace)?.1;
        Ok(Block {
            statements,
            return_value,
            return_span,
            span: start.merge(end),
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, FrontendError> {
        match self.current_token() {
            Token::Let => Ok(Stmt::Let(self.parse_let_stmt()?)),
            Token::Assert => Ok(Stmt::Assert(self.parse_assert_stmt()?)),
            Token::Emit => Ok(Stmt::Emit(self.parse_emit_stmt()?)),
            Token::If => Ok(Stmt::If(Box::new(self.parse_if_stmt()?))),
            Token::Match => Ok(Stmt::Match(self.parse_match_stmt()?)),
            Token::Requires => Err(self.deferred_feature(DeferredSyntaxFeature::Requires)),
            Token::Ensures => Err(self.deferred_feature(DeferredSyntaxFeature::Ensures)),
            Token::For => Err(self.deferred_feature(DeferredSyntaxFeature::ForLoop)),
            Token::Predicate => Err(self.deferred_feature(DeferredSyntaxFeature::Predicate)),
            Token::Invariant => Err(self.deferred_feature(DeferredSyntaxFeature::Invariant)),
            _ => self.parse_assign_or_expr_stmt(),
        }
    }

    fn parse_let_stmt(&mut self) -> Result<LetStmt, FrontendError> {
        let start = self.expect(&Token::Let)?.1;
        let pattern = self.parse_pattern()?;
        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(LetStmt {
            pattern,
            value,
            span: start.merge(end),
        })
    }

    fn parse_assert_stmt(&mut self) -> Result<AssertStmt, FrontendError> {
        let start = self.expect(&Token::Assert)?.1;
        if self.at(&Token::Relation) {
            self.bump();
            let relation = self.parse_path()?;
            let args = self.parse_call_args()?;
            let end = self.expect(&Token::Semi)?.1;
            Ok(AssertStmt::Relation {
                relation,
                args,
                span: start.merge(end),
            })
        } else {
            let expr = self.parse_expr()?;
            let end = self.expect(&Token::Semi)?.1;
            Ok(AssertStmt::Expr {
                expr,
                span: start.merge(end),
            })
        }
    }

    fn parse_emit_stmt(&mut self) -> Result<EmitStmt, FrontendError> {
        let start = self.expect(&Token::Emit)?.1;
        let event = self.parse_path()?;
        let args = self.parse_call_args()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(EmitStmt {
            event,
            args,
            span: start.merge(end),
        })
    }

    fn parse_if_stmt(&mut self) -> Result<IfStmt, FrontendError> {
        let start = self.expect(&Token::If)?.1;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_block = if self.at(&Token::Else) {
            self.bump();
            if self.at(&Token::If) {
                return Err(self.deferred_feature(DeferredSyntaxFeature::ElseIf));
            }
            Some(self.parse_block()?)
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map_or(then_block.span, |block| block.span);
        Ok(IfStmt {
            cond,
            then_block,
            else_block,
            span: start.merge(end),
        })
    }

    fn parse_match_stmt(&mut self) -> Result<MatchStmt, FrontendError> {
        let start = self.expect(&Token::Match)?.1;
        let scrutinee = self.parse_expr()?;
        self.expect(&Token::LBrace)?;
        let mut arms = Vec::new();
        while !self.at(&Token::RBrace) {
            let arm_start = self.current_span();
            let pattern = self.parse_match_pattern()?;
            if self.at(&Token::If) {
                return Err(self.deferred_feature(DeferredSyntaxFeature::MatchGuard));
            }
            self.expect(&Token::FatArrow)?;
            if !self.at(&Token::LBrace) {
                return Err(self.deferred_feature(DeferredSyntaxFeature::ExpressionMatchArm));
            }
            let block = self.parse_block()?;
            arms.push(MatchArm {
                pattern,
                span: arm_start.merge(block.span),
                block,
            });
        }
        if arms.is_empty() {
            return Err(self.error_here(
                FrontendErrorKind::InvalidProgram,
                "match must contain at least one arm",
            ));
        }
        let end = self.expect(&Token::RBrace)?.1;
        Ok(MatchStmt {
            scrutinee,
            arms,
            span: start.merge(end),
        })
    }

    fn parse_match_pattern(&mut self) -> Result<MatchPattern, FrontendError> {
        match self.current_token() {
            Token::True => {
                let span = self.bump().1;
                Ok(MatchPattern::Literal(LiteralExpr {
                    kind: LiteralKind::Bool(true),
                    span,
                }))
            }
            Token::False => {
                let span = self.bump().1;
                Ok(MatchPattern::Literal(LiteralExpr {
                    kind: LiteralKind::Bool(false),
                    span,
                }))
            }
            Token::IntLit(value) => {
                let span = self.bump().1;
                Ok(MatchPattern::Literal(LiteralExpr {
                    kind: LiteralKind::Integer(value),
                    span,
                }))
            }
            Token::HexLit(bytes) => {
                let span = self.bump().1;
                Ok(MatchPattern::Literal(LiteralExpr {
                    kind: LiteralKind::Bytes32(bytes),
                    span,
                }))
            }
            Token::Ident(name) if name == "_" => {
                let span = self.bump().1;
                Ok(MatchPattern::Wildcard(span))
            }
            Token::Ident(_) => Err(self.deferred_feature(DeferredSyntaxFeature::PathMatchPattern)),
            Token::LParen => Err(self.deferred_feature(DeferredSyntaxFeature::TupleMatchPattern)),
            _ => Err(self.error_here(
                FrontendErrorKind::UnexpectedToken,
                "expected literal pattern or _",
            )),
        }
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Result<Stmt, FrontendError> {
        if let Some(assign) = self.try_parse_state_assign()? {
            return Ok(Stmt::StateAssign(assign));
        }
        let start = self.current_span();
        let expr = self.parse_expr()?;
        let end = self.expect(&Token::Semi)?.1;
        Ok(Stmt::Expr(ExprStmt {
            expr,
            span: start.merge(end),
        }))
    }

    fn try_parse_state_assign(&mut self) -> Result<Option<StateAssignStmt>, FrontendError> {
        let checkpoint = self.pos;
        let Some(table) = self.try_parse_path() else {
            self.pos = checkpoint;
            return Ok(None);
        };
        let result = (|| -> Result<StateAssignStmt, FrontendError> {
            self.expect(&Token::LBracket)?;
            let key = self.parse_expr_list(&Token::RBracket)?;
            self.expect(&Token::RBracket)?;
            self.expect(&Token::Dot)?;
            let (field, field_span) = self.expect_ident()?;
            self.expect(&Token::Eq)?;
            let value = self.parse_expr()?;
            let end = self.expect(&Token::Semi)?.1;
            Ok(StateAssignStmt {
                span: table.span.merge(end),
                table,
                key,
                field,
                field_span,
                value,
            })
        })();
        match result {
            Ok(assign) => Ok(Some(assign)),
            Err(_) => {
                self.pos = checkpoint;
                Ok(None)
            }
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, FrontendError> {
        if self.at(&Token::LParen) {
            let start = self.bump().1;
            let mut names = Vec::new();
            let (first, span) = self.expect_ident()?;
            names.push((first, span));
            while self.at(&Token::Comma) {
                self.bump();
                let (name, span) = self.expect_ident()?;
                names.push((name, span));
            }
            let end = self.expect(&Token::RParen)?.1;
            Ok(Pattern::Tuple(names, start.merge(end)))
        } else {
            let (name, span) = self.expect_ident()?;
            Ok(Pattern::Name(name, span))
        }
    }

    fn parse_param_list(&mut self) -> Result<Vec<ParamDecl>, FrontendError> {
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            params.push(self.parse_param_decl()?);
            while self.at(&Token::Comma) {
                self.bump();
                params.push(self.parse_param_decl()?);
            }
        }
        self.expect(&Token::RParen)?;
        Ok(params)
    }

    fn parse_param_decl(&mut self) -> Result<ParamDecl, FrontendError> {
        let (symbol, start) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type_expr()?;
        Ok(ParamDecl {
            symbol,
            span: start.merge(ty.path.span),
            ty,
        })
    }

    fn parse_result_list(&mut self) -> Result<Vec<ResultDecl>, FrontendError> {
        let mut results = vec![self.parse_result_decl()?];
        while self.at(&Token::Comma) {
            self.bump();
            results.push(self.parse_result_decl()?);
        }
        Ok(results)
    }

    fn parse_result_decl(&mut self) -> Result<ResultDecl, FrontendError> {
        let (symbol, start) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let ty = self.parse_type_expr()?;
        Ok(ResultDecl {
            symbol,
            span: start.merge(ty.path.span),
            ty,
        })
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, FrontendError> {
        Ok(TypeExpr {
            path: self.parse_path()?,
        })
    }

    fn parse_path(&mut self) -> Result<IdentPath, FrontendError> {
        let mut segments = Vec::new();
        let (first, start) = self.expect_ident()?;
        let mut end = start;
        segments.push(first);
        while self.at(&Token::PathSep) {
            self.bump();
            let (segment, segment_span) = self.expect_ident()?;
            end = segment_span;
            segments.push(segment);
        }
        Ok(IdentPath {
            segments,
            span: start.merge(end),
        })
    }

    fn try_parse_path(&mut self) -> Option<IdentPath> {
        let checkpoint = self.pos;
        let path = self.parse_path().ok();
        if path.is_none() {
            self.pos = checkpoint;
        }
        path
    }

    fn parse_tuple_like(&mut self) -> Result<Vec<Expr>, FrontendError> {
        if self.at(&Token::LParen) {
            self.bump();
            let values = self.parse_expr_list(&Token::RParen)?;
            self.expect(&Token::RParen)?;
            Ok(values)
        } else {
            Ok(vec![self.parse_expr()?])
        }
    }

    fn parse_delimited_exprs(&mut self, terminator: &Token) -> Result<Vec<Expr>, FrontendError> {
        if self.at(terminator) {
            return Ok(Vec::new());
        }
        let mut values = vec![self.parse_expr()?];
        while self.at(&Token::Comma) {
            self.bump();
            if self.at(terminator) {
                break;
            }
            values.push(self.parse_expr()?);
        }
        Ok(values)
    }

    fn parse_expr_list(&mut self, terminator: &Token) -> Result<Vec<Expr>, FrontendError> {
        self.parse_delimited_exprs(terminator)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, FrontendError> {
        self.expect(&Token::LParen)?;
        let values = self.parse_expr_list(&Token::RParen)?;
        self.expect(&Token::RParen)?;
        Ok(values)
    }

    fn parse_expr(&mut self) -> Result<Expr, FrontendError> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, FrontendError> {
        let mut lhs = self.parse_prefix_expr()?;
        loop {
            let (left_bp, right_bp, op) = match self.current_token() {
                Token::PipePipe => (1, 2, BinaryOp::Or),
                Token::AmpAmp => (3, 4, BinaryOp::And),
                Token::EqEq => (5, 6, BinaryOp::Eq),
                Token::BangEq => (5, 6, BinaryOp::Ne),
                Token::Lt => (7, 8, BinaryOp::Lt),
                Token::LtEq => (7, 8, BinaryOp::Le),
                Token::Gt => (7, 8, BinaryOp::Gt),
                Token::GtEq => (7, 8, BinaryOp::Ge),
                Token::Plus => (9, 10, BinaryOp::Add),
                Token::Minus => (9, 10, BinaryOp::Sub),
                Token::Star => (11, 12, BinaryOp::Mul),
                Token::Slash => (11, 12, BinaryOp::Div),
                Token::Percent => (11, 12, BinaryOp::Mod),
                _ => break,
            };
            if left_bp < min_bp {
                break;
            }
            let _ = self.bump();
            let rhs = self.parse_expr_bp(right_bp)?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary(BinaryExpr {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            });
        }
        Ok(lhs)
    }

    fn parse_prefix_expr(&mut self) -> Result<Expr, FrontendError> {
        match self.current_token() {
            Token::Bang => {
                let start = self.bump().1;
                let expr = self.parse_expr_bp(13)?;
                Ok(Expr::Unary(UnaryExpr {
                    op: UnaryOp::Not,
                    span: start.merge(expr.span()),
                    expr: Box::new(expr),
                }))
            }
            Token::Minus => {
                let start = self.bump().1;
                let expr = self.parse_expr_bp(13)?;
                Ok(Expr::Unary(UnaryExpr {
                    op: UnaryOp::Neg,
                    span: start.merge(expr.span()),
                    expr: Box::new(expr),
                }))
            }
            Token::Eval => self.parse_eval_relation_expr(),
            Token::Select => self.parse_select_expr(),
            Token::True => {
                let span = self.bump().1;
                Ok(Expr::Literal(LiteralExpr {
                    kind: LiteralKind::Bool(true),
                    span,
                }))
            }
            Token::False => {
                let span = self.bump().1;
                Ok(Expr::Literal(LiteralExpr {
                    kind: LiteralKind::Bool(false),
                    span,
                }))
            }
            Token::IntLit(value) => {
                let span = self.bump().1;
                Ok(Expr::Literal(LiteralExpr {
                    kind: LiteralKind::Integer(value),
                    span,
                }))
            }
            Token::HexLit(bytes) => {
                let span = self.bump().1;
                Ok(Expr::Literal(LiteralExpr {
                    kind: LiteralKind::Bytes32(bytes),
                    span,
                }))
            }
            Token::LParen => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Ident(_) => self.parse_path_led_expr(),
            _ => Err(self.error_here(FrontendErrorKind::UnexpectedToken, "expected expression")),
        }
    }

    fn parse_eval_relation_expr(&mut self) -> Result<Expr, FrontendError> {
        let start = self.expect(&Token::Eval)?.1;
        self.expect(&Token::Relation)?;
        let relation = self.parse_path()?;
        let args = self.parse_call_args()?;
        let span = start.merge(self.current_span());
        Ok(Expr::EvalRelation(EvalRelationExpr {
            relation,
            args,
            span,
        }))
    }

    fn parse_select_expr(&mut self) -> Result<Expr, FrontendError> {
        let start = self.expect(&Token::Select)?.1;
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Comma)?;
        let if_true = self.parse_expr()?;
        self.expect(&Token::Comma)?;
        let if_false = self.parse_expr()?;
        let end = self.expect(&Token::RParen)?.1;
        Ok(Expr::Select(SelectExpr {
            cond: Box::new(cond),
            if_true: Box::new(if_true),
            if_false: Box::new(if_false),
            span: start.merge(end),
        }))
    }

    fn parse_path_led_expr(&mut self) -> Result<Expr, FrontendError> {
        let path = self.parse_path()?;
        if self.at(&Token::LParen) {
            let args = self.parse_call_args()?;
            let span = path.span.merge(self.current_span());
            return Ok(Expr::Call(CallExpr {
                callee: path,
                args,
                span,
            }));
        }
        if self.at(&Token::LBracket) {
            self.bump();
            let key = self.parse_expr_list(&Token::RBracket)?;
            self.expect(&Token::RBracket)?;
            self.expect(&Token::Dot)?;
            let (field, field_span) = self.expect_ident()?;
            let span = path.span.merge(field_span);
            return Ok(Expr::TableRead(TableReadExpr {
                table: path,
                key,
                field,
                field_span,
                span,
            }));
        }
        Ok(Expr::Name(path))
    }

    fn current_token(&self) -> Token {
        self.tokens[self.pos].0.clone()
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].1
    }

    fn at(&self, token: &Token) -> bool {
        std::mem::discriminant(&self.tokens[self.pos].0) == std::mem::discriminant(token)
    }

    fn bump(&mut self) -> (Token, Span) {
        let item = self.tokens[self.pos].clone();
        self.pos += 1;
        item
    }

    fn expect(&mut self, token: &Token) -> Result<(Token, Span), FrontendError> {
        if self.at(token) {
            Ok(self.bump())
        } else {
            Err(self.error_here(
                FrontendErrorKind::ExpectedToken,
                format!("expected {token:?}"),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), FrontendError> {
        match self.bump() {
            (Token::Ident(value), span) => Ok((value, span)),
            (_, span) => Err(FrontendError::new(
                FrontendErrorKind::ExpectedToken,
                span,
                "expected identifier",
            )),
        }
    }

    fn error_here(&self, kind: FrontendErrorKind, message: impl Into<String>) -> FrontendError {
        FrontendError::new(kind, self.current_span(), message)
    }

    fn deferred_feature(&self, feature: DeferredSyntaxFeature) -> FrontendError {
        deferred_feature_error(self.current_span(), feature)
    }
}
