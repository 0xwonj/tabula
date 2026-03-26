use crate::span::Span;

use super::{Block, Expr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub symbol: String,
    pub uses: Vec<UseDecl>,
    pub decls: Vec<TopDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentPath {
    pub segments: Vec<String>,
    pub span: Span,
}

impl IdentPath {
    pub fn as_string(&self) -> String {
        self.segments.join("::")
    }

    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
    pub path: IdentPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub path: IdentPath,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopDecl {
    Context(ContextDecl),
    State(StateDecl),
    Const(ConstDecl),
    Relation(RelationDecl),
    Event(EventDecl),
    Callable(CallableDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDecl {
    pub fields: Vec<ContextFieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFieldDecl {
    pub symbol: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub tables: Vec<TableDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDecl {
    pub symbol: String,
    pub keys: Vec<ParamDecl>,
    pub fields: Vec<StateFieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldDecl {
    pub symbol: String,
    pub ty: TypeExpr,
    pub scheme: Option<IdentPath>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    pub symbol: String,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationDecl {
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub results: Vec<ResultDecl>,
    pub body: RelationBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationBody {
    Enum {
        values: Vec<Expr>,
        span: Span,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span,
    },
    Map {
        entries: Vec<RelationMapEntry>,
        span: Span,
    },
    Set {
        tuples: Vec<Vec<Expr>>,
        span: Span,
    },
    Extern {
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationMapEntry {
    pub inputs: Vec<Expr>,
    pub outputs: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub symbol: String,
    pub fields: Vec<ParamDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    pub symbol: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultDecl {
    pub symbol: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    Function,
    Query,
    Tx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableDecl {
    pub kind: CallableKind,
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeExpr>,
    pub body: Block,
    pub span: Span,
}
