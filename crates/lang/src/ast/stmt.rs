use crate::span::Span;

use super::{Expr, IdentPath, LiteralExpr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub return_value: Option<Expr>,
    pub return_span: Option<Span>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(LetStmt),
    StateAssign(StateAssignStmt),
    Assert(AssertStmt),
    Emit(EmitStmt),
    If(Box<IfStmt>),
    Match(MatchStmt),
    Expr(ExprStmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetStmt {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Name(String, Span),
    Tuple(Vec<(String, Span)>, Span),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAssignStmt {
    pub table: IdentPath,
    pub key: Vec<Expr>,
    pub field: String,
    pub field_span: Span,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertStmt {
    Expr {
        expr: Expr,
        span: Span,
    },
    Relation {
        relation: IdentPath,
        args: Vec<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitStmt {
    pub event: IdentPath,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_block: Block,
    pub else_block: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub block: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    Literal(LiteralExpr),
    Wildcard(Span),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}
