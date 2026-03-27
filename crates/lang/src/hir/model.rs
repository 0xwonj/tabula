//! HIR model types: the resolved in-memory representation after building.
#![allow(missing_docs)]

use tabula_core::{PortableValue, SchemeId};

use crate::span::Span;

#[allow(clippy::wildcard_imports)]
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub id: CapabilityRefId,
    pub symbol: String,
    pub path: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub totality: CapabilityTotality,
    pub query_policy: CapabilityQueryPolicy,
    pub proof_visibility: CapabilityProofVisibility,
    pub hash_family: Option<HashFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub symbol: String,
    pub uses: Vec<UseDecl>,
    pub context: Option<ContextDecl>,
    pub state: Option<StateDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct VerifiedProgram(pub(crate) Program);

impl VerifiedProgram {
    pub fn program(&self) -> &Program {
        &self.0
    }

    pub fn into_program(self) -> Program {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub capability: CapabilityDescriptor,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDecl {
    pub fields: Vec<ContextFieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFieldDecl {
    pub id: ContextFieldId,
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub tables: Vec<TableDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDecl {
    pub id: TableId,
    pub symbol: String,
    pub keys: Vec<ParamDecl>,
    pub fields: Vec<StateFieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldDecl {
    pub id: FieldId,
    pub symbol: String,
    pub ty: TypeRef,
    pub scheme: Option<SchemeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeRef {
    pub id: SchemeId,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Const(ConstDecl),
    Relation(RelationDecl),
    Event(EventDecl),
    Callable(CallableDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    pub id: ParamId,
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultDecl {
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    pub id: ConstId,
    pub symbol: String,
    pub ty: TypeRef,
    pub value: ConstExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationDecl {
    pub id: RelationId,
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub results: Vec<ResultDecl>,
    pub body: RelationBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationBody {
    Enum { values: Vec<ConstExpr> },
    Range { start: ConstExpr, end: ConstExpr },
    Map { entries: Vec<RelationMapEntry> },
    Set { tuples: Vec<Vec<ConstExpr>> },
    Extern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationMapEntry {
    pub inputs: Vec<ConstExpr>,
    pub outputs: Vec<ConstExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub id: EventId,
    pub symbol: String,
    pub fields: Vec<ParamDecl>,
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
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: Body,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub region: Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub statements: Vec<Stmt>,
    pub terminator: Terminator,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Return { values: Vec<Expr>, span: Span },
    Yield { values: Vec<Expr>, span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(LetStmt),
    StateAssign(StateAssignStmt),
    Assert(AssertStmt),
    Emit(EmitStmt),
    If(IfStmt),
    Match(MatchStmt),
    Expr(ExprStmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingDecl {
    pub id: BindingId,
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetStmt {
    pub binding: BindingDecl,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAssignStmt {
    pub target: StatePlace,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePlace {
    pub table: TableId,
    pub key: Vec<Expr>,
    pub field: FieldId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertStmt {
    Expr {
        expr: Expr,
        span: Span,
    },
    Relation {
        relation: RelationId,
        args: Vec<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitStmt {
    pub event: EventId,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then_region: Region,
    pub else_region: Region,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub default: Option<Region>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub region: Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    Literal(PortableValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(LiteralExpr),
    Local(LocalRefExpr),
    Context(ContextRefExpr),
    Const(ConstRefExpr),
    TableRead(TableReadExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    CallFunction(CallFunctionExpr),
    CallCapability(CallCapabilityExpr),
    Hash(HashExpr),
    EvalRelation(EvalRelationExpr),
    Select(SelectExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Literal(expr) => expr.span,
            Self::Local(expr) => expr.span,
            Self::Context(expr) => expr.span,
            Self::Const(expr) => expr.span,
            Self::TableRead(expr) => expr.span,
            Self::Unary(expr) => expr.span,
            Self::Binary(expr) => expr.span,
            Self::CallFunction(expr) => expr.span,
            Self::CallCapability(expr) => expr.span,
            Self::Hash(expr) => expr.span,
            Self::EvalRelation(expr) => expr.span,
            Self::Select(expr) => expr.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralExpr {
    pub value: PortableValue,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalRef {
    Param(ParamId),
    Binding(BindingId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRefExpr {
    pub local: LocalRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextRefExpr {
    pub field: ContextFieldId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstRefExpr {
    pub const_id: ConstId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableReadExpr {
    pub table: TableId,
    pub key: Vec<Expr>,
    pub field: FieldId,
    pub ty: TypeRef,
    pub span: Span,
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
    pub ty: TypeRef,
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
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFunctionExpr {
    pub callee: CallableId,
    pub args: Vec<Expr>,
    pub returns: Vec<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallCapabilityExpr {
    pub capability: CapabilityRefId,
    pub args: Vec<Expr>,
    pub outputs: Vec<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashExpr {
    pub family: HashFamily,
    pub inputs: Vec<TypeRef>,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRelationExpr {
    pub relation: RelationId,
    pub args: Vec<Expr>,
    pub outputs: Vec<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectExpr {
    pub cond: Box<Expr>,
    pub if_true: Box<Expr>,
    pub if_false: Box<Expr>,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstExpr {
    Literal(PortableValue),
    Unary {
        op: UnaryOp,
        expr: Box<ConstExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<ConstExpr>,
        rhs: Box<ConstExpr>,
    },
}
