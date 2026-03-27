use crate::span::Span;

use super::IdentPath;

/// A source-level expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// An integer, boolean, or bytes32 literal.
    Literal(LiteralExpr),
    /// An identifier or path reference.
    Name(IdentPath),
    /// A unary operator expression.
    Unary(UnaryExpr),
    /// A binary operator expression.
    Binary(BinaryExpr),
    /// A function or query call.
    Call(CallExpr),
    /// A state table field read.
    TableRead(TableReadExpr),
    /// An evaluate-relation expression.
    EvalRelation(EvalRelationExpr),
    /// A ternary select expression.
    Select(SelectExpr),
}

impl Expr {
    /// Return the source span of this expression.
    pub fn span(&self) -> Span {
        match self {
            Self::Literal(expr) => expr.span,
            Self::Name(path) => path.span,
            Self::Unary(expr) => expr.span,
            Self::Binary(expr) => expr.span,
            Self::Call(expr) => expr.span,
            Self::TableRead(expr) => expr.span,
            Self::EvalRelation(expr) => expr.span,
            Self::Select(expr) => expr.span,
        }
    }
}

/// A literal expression with an associated source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralExpr {
    /// The literal value.
    pub kind: LiteralKind,
    /// Source span.
    pub span: Span,
}

/// The value of a literal expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralKind {
    /// An unsigned 64-bit integer literal.
    Integer(u64),
    /// A boolean literal.
    Bool(bool),
    /// A 32-byte hex literal.
    Bytes32([u8; 32]),
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical NOT.
    Not,
    /// Arithmetic negation.
    Neg,
}

/// A unary operator applied to one sub-expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnaryExpr {
    /// The operator.
    pub op: UnaryOp,
    /// The operand.
    pub expr: Box<Expr>,
    /// Source span.
    pub span: Span,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Integer division.
    Div,
    /// Remainder.
    Mod,
    /// Equality.
    Eq,
    /// Inequality.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Logical AND.
    And,
    /// Logical OR.
    Or,
}

/// A binary operator applied to two sub-expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExpr {
    /// The operator.
    pub op: BinaryOp,
    /// Left operand.
    pub lhs: Box<Expr>,
    /// Right operand.
    pub rhs: Box<Expr>,
    /// Source span.
    pub span: Span,
}

/// A function or query call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallExpr {
    /// Path to the callee.
    pub callee: IdentPath,
    /// Call arguments.
    pub args: Vec<Expr>,
    /// Source span.
    pub span: Span,
}

/// A state table field read expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReadExpr {
    /// Path to the table.
    pub table: IdentPath,
    /// Key expressions forming the row address.
    pub key: Vec<Expr>,
    /// Name of the column field to read.
    pub field: String,
    /// Source span of the field name.
    pub field_span: Span,
    /// Source span of the entire expression.
    pub span: Span,
}

/// An evaluate-relation expression (returns the relation output values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRelationExpr {
    /// Path to the relation.
    pub relation: IdentPath,
    /// Input arguments.
    pub args: Vec<Expr>,
    /// Source span.
    pub span: Span,
}

/// A ternary select expression: `if cond { if_true } else { if_false }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectExpr {
    /// Condition expression.
    pub cond: Box<Expr>,
    /// Value when condition is true.
    pub if_true: Box<Expr>,
    /// Value when condition is false.
    pub if_false: Box<Expr>,
    /// Source span.
    pub span: Span,
}
