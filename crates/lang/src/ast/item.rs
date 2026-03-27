use crate::span::Span;

use super::{Block, Expr};

/// Root node of a parsed Tabula source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Program symbol (name declared at the top of the source file).
    pub symbol: String,
    /// Capability `use` declarations.
    pub uses: Vec<UseDecl>,
    /// Top-level declarations (state, context, callables, relations, events, consts).
    pub decls: Vec<TopDecl>,
    /// Source span of the entire program.
    pub span: Span,
}

/// A `::` separated identifier path (e.g. `foo::bar::Baz`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentPath {
    /// Path segments in order.
    pub segments: Vec<String>,
    /// Source span.
    pub span: Span,
}

impl IdentPath {
    /// Join segments with `::` into a single string.
    pub fn as_string(&self) -> String {
        self.segments.join("::")
    }

    /// Return the last segment, if any.
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

/// A type reference at the source level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
    /// Identifier path naming the type.
    pub path: IdentPath,
}

/// A `use` declaration for a native capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    /// Path to the capability.
    pub path: IdentPath,
    /// Source span.
    pub span: Span,
}

/// A top-level declaration in a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopDecl {
    /// The `context` block.
    Context(ContextDecl),
    /// The `state` block.
    State(StateDecl),
    /// A compile-time constant.
    Const(ConstDecl),
    /// A static relation.
    Relation(RelationDecl),
    /// An event type.
    Event(EventDecl),
    /// A function, query, or transaction.
    Callable(CallableDecl),
}

/// The `context` block declaring public inputs supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDecl {
    /// Fields of the context block.
    pub fields: Vec<ContextFieldDecl>,
    /// Source span.
    pub span: Span,
}

/// A single field in the `context` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFieldDecl {
    /// Field name.
    pub symbol: String,
    /// Field type.
    pub ty: TypeExpr,
    /// Source span.
    pub span: Span,
}

/// The `state` block declaring mutable tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    /// Tables declared in the state block.
    pub tables: Vec<TableDecl>,
    /// Source span.
    pub span: Span,
}

/// A single table within the `state` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDecl {
    /// Table name.
    pub symbol: String,
    /// Key column declarations (form the composite primary key).
    pub keys: Vec<ParamDecl>,
    /// Non-key value column declarations.
    pub fields: Vec<StateFieldDecl>,
    /// Source span.
    pub span: Span,
}

/// A single value column in a state table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldDecl {
    /// Column name.
    pub symbol: String,
    /// Column value type.
    pub ty: TypeExpr,
    /// Optional column commitment scheme binding.
    pub scheme: Option<IdentPath>,
    /// Source span.
    pub span: Span,
}

/// A compile-time constant declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    /// Constant name.
    pub symbol: String,
    /// Declared type.
    pub ty: TypeExpr,
    /// Initializer expression (must be a constant).
    pub value: Expr,
    /// Source span.
    pub span: Span,
}

/// A static relation declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationDecl {
    /// Relation name.
    pub symbol: String,
    /// Input parameter types.
    pub params: Vec<ParamDecl>,
    /// Output result types.
    pub results: Vec<ResultDecl>,
    /// How the relation data is bound.
    pub body: RelationBody,
    /// Source span.
    pub span: Span,
}

/// The body of a relation declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationBody {
    /// An enumerated set of values.
    Enum {
        /// The enumerated values.
        values: Vec<Expr>,
        /// Source span.
        span: Span,
    },
    /// An integer range `start..end`.
    Range {
        /// Inclusive lower bound.
        start: Box<Expr>,
        /// Exclusive upper bound.
        end: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// An explicit (inputs → outputs) mapping.
    Map {
        /// Map rows.
        entries: Vec<RelationMapEntry>,
        /// Source span.
        span: Span,
    },
    /// A set of input tuples (no outputs).
    Set {
        /// Input tuples.
        tuples: Vec<Vec<Expr>>,
        /// Source span.
        span: Span,
    },
    /// An externally supplied relation (no static data).
    Extern {
        /// Source span.
        span: Span,
    },
}

/// A single (inputs → outputs) row in a map relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationMapEntry {
    /// Input values for this row.
    pub inputs: Vec<Expr>,
    /// Output values for this row.
    pub outputs: Vec<Expr>,
    /// Source span.
    pub span: Span,
}

/// An event type declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    /// Event name.
    pub symbol: String,
    /// Event field types.
    pub fields: Vec<ParamDecl>,
    /// Source span.
    pub span: Span,
}

/// A single parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    /// Parameter name.
    pub symbol: String,
    /// Parameter type.
    pub ty: TypeExpr,
    /// Source span.
    pub span: Span,
}

/// A single result declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultDecl {
    /// Result name.
    pub symbol: String,
    /// Result type.
    pub ty: TypeExpr,
    /// Source span.
    pub span: Span,
}

/// Discriminates callable declaration kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    /// A pure function (no state side-effects).
    Function,
    /// A read-only query.
    Query,
    /// A state-mutating transaction.
    Tx,
}

/// A function, query, or transaction declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableDecl {
    /// Whether this is a function, query, or tx.
    pub kind: CallableKind,
    /// Callable name.
    pub symbol: String,
    /// Input parameters.
    pub params: Vec<ParamDecl>,
    /// Return types.
    pub returns: Vec<TypeExpr>,
    /// Body block.
    pub body: Block,
    /// Source span.
    pub span: Span,
}
