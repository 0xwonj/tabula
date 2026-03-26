use crate::span::Span;

use super::IdentPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(LiteralExpr),
    Name(IdentPath),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Call(CallExpr),
    TableRead(TableReadExpr),
    EvalRelation(EvalRelationExpr),
    Select(SelectExpr),
}

impl Expr {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralExpr {
    pub kind: LiteralKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralKind {
    Integer(u64),
    Bool(bool),
    Bytes32([u8; 32]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallExpr {
    pub callee: IdentPath,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReadExpr {
    pub table: IdentPath,
    pub key: Vec<Expr>,
    pub field: String,
    pub field_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRelationExpr {
    pub relation: IdentPath,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectExpr {
    pub cond: Box<Expr>,
    pub if_true: Box<Expr>,
    pub if_false: Box<Expr>,
    pub span: Span,
}
