use crate::span::Span;

use super::{Expr, IdentPath, LiteralExpr};

/// A block of statements with an optional trailing return expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Statements in the block.
    pub statements: Vec<Stmt>,
    /// Optional return value (last expression in the block).
    pub return_value: Option<Expr>,
    /// Source span of the return expression, if present.
    pub return_span: Option<Span>,
    /// Source span of the entire block.
    pub span: Span,
}

/// A statement in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// A `let` binding.
    Let(LetStmt),
    /// A state table field write.
    StateAssign(StateAssignStmt),
    /// An assertion.
    Assert(AssertStmt),
    /// An event emission.
    Emit(EmitStmt),
    /// An `if` / `else` branch.
    If(Box<IfStmt>),
    /// A `match` expression statement.
    Match(MatchStmt),
    /// An expression used as a statement.
    Expr(ExprStmt),
}

/// A `let` binding statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetStmt {
    /// The binding pattern.
    pub pattern: Pattern,
    /// The initializer expression.
    pub value: Expr,
    /// Source span.
    pub span: Span,
}

/// A let-binding pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// A single name binding: `let x = …`.
    Name(String, Span),
    /// A tuple destructuring: `let (a, b) = …`.
    Tuple(Vec<(String, Span)>, Span),
}

/// A state table field write statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAssignStmt {
    /// Path to the target table.
    pub table: IdentPath,
    /// Key expressions forming the row address.
    pub key: Vec<Expr>,
    /// Target column field name.
    pub field: String,
    /// Source span of the field name.
    pub field_span: Span,
    /// Value to write.
    pub value: Expr,
    /// Source span.
    pub span: Span,
}

/// An assertion statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertStmt {
    /// Assert an arbitrary boolean expression.
    Expr {
        /// The condition that must be true.
        expr: Expr,
        /// Source span.
        span: Span,
    },
    /// Assert that a tuple satisfies a static relation.
    Relation {
        /// Path to the relation.
        relation: IdentPath,
        /// Input arguments.
        args: Vec<Expr>,
        /// Source span.
        span: Span,
    },
}

/// An event emission statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitStmt {
    /// Path to the event type.
    pub event: IdentPath,
    /// Event field arguments.
    pub args: Vec<Expr>,
    /// Source span.
    pub span: Span,
}

/// An `if` / `else` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    /// Branch condition.
    pub cond: Expr,
    /// Block executed when the condition is true.
    pub then_block: Block,
    /// Optional block executed when the condition is false.
    pub else_block: Option<Block>,
    /// Source span.
    pub span: Span,
}

/// A `match` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStmt {
    /// The value being matched.
    pub scrutinee: Expr,
    /// Match arms in source order.
    pub arms: Vec<MatchArm>,
    /// Source span.
    pub span: Span,
}

/// A single arm of a `match` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The pattern this arm matches.
    pub pattern: MatchPattern,
    /// Block executed when the pattern matches.
    pub block: Block,
    /// Source span.
    pub span: Span,
}

/// A match pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    /// A literal value pattern.
    Literal(LiteralExpr),
    /// A wildcard (`_`) that matches anything.
    Wildcard(Span),
}

/// An expression used as a statement (value is discarded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprStmt {
    /// The expression.
    pub expr: Expr,
    /// Source span.
    pub span: Span,
}
