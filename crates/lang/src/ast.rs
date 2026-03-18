//! Abstract syntax tree for the Tabula DSL.
//!
//! AST types use self-documenting variant names; field-level docs are omitted
//! where the name is unambiguous.

use crate::span::Span;

/// A complete Tabula program: table declarations + transaction definitions.
#[derive(Debug, Clone)]
pub struct Program {
    /// Table schema declarations.
    pub tables: Vec<TableDecl>,
    /// Transaction type definitions.
    pub transactions: Vec<TxDecl>,
}

// ---------------------------------------------------------------------------
// Top-level declarations
// ---------------------------------------------------------------------------

/// A table schema declaration.
#[derive(Debug, Clone)]
pub struct TableDecl {
    /// Table name.
    pub name: String,
    /// Column definitions.
    pub columns: Vec<ColumnDecl>,
    /// Source span covering the entire declaration.
    pub span: Span,
}

/// A column within a table declaration.
#[derive(Debug, Clone)]
pub struct ColumnDecl {
    /// Column name.
    pub name: String,
    /// Column type.
    pub ty: TypeName,
    /// Optional column commitment scheme annotation.
    pub scheme: Option<ColumnSchemeDecl>,
    /// Source span.
    pub span: Span,
}

/// A column commitment scheme as written in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnSchemeDecl {
    /// Built-in sorted-state Merkle commitment.
    Ssmc,
    /// Built-in sparse Merkle tree commitment.
    Smt,
    /// Explicit numeric scheme identifier.
    Numeric(u16),
}

/// A type name as written in source.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    U64,
    I64,
    Bool,
    Bytes32,
}

/// A transaction type definition.
#[derive(Debug, Clone)]
pub struct TxDecl {
    /// Transaction name.
    pub name: String,
    /// Parameter declarations.
    pub params: Vec<ParamDecl>,
    /// Body statements.
    pub body: Vec<Stmt>,
    /// Source span covering the entire definition.
    pub span: Span,
}

/// A parameter in a transaction definition.
#[derive(Debug, Clone)]
pub struct ParamDecl {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub ty: TypeName,
    /// Source span.
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

/// A statement within a transaction body.
#[derive(Debug, Clone)]
pub struct Stmt {
    /// The statement content.
    pub kind: StmtKind,
    /// Source span.
    pub span: Span,
}

/// Statement variants.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `let name = expr`
    Let { name: String, value: Expr },
    /// `let (a, b) = divmod(lhs, rhs)`
    LetDestructure {
        first: String,
        second: String,
        lhs: Expr,
        rhs: Expr,
    },
    /// `table[row].col = expr`
    Assign {
        table: String,
        row: Expr,
        col: String,
        value: Expr,
    },
    /// `assert expr`
    Assert { condition: Expr },
    /// `emit "topic" (args...)`
    Emit { topic: String, args: Vec<Expr> },
    /// `@precompile(0x0001, [dst1, dst2], arg1, arg2)`
    Precompile {
        id: u16,
        dst_names: Vec<String>,
        inputs: Vec<Expr>,
    },
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

/// An expression with source location.
#[derive(Debug, Clone)]
pub struct Expr {
    /// Expression content.
    pub kind: ExprKind,
    /// Source span.
    pub span: Span,
}

/// Expression variants.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum ExprKind {
    /// Integer literal (non-negative).
    IntLit(u64),
    /// Boolean literal.
    BoolLit(bool),
    /// 32-byte hex literal.
    HexLit([u8; 32]),
    /// `null`
    Null,
    /// Variable reference (parameter or local binding).
    Ident(String),
    /// Cell read: `table[row].col`
    CellRead {
        table: String,
        row: Box<Expr>,
        col: String,
    },
    /// Static table read: `@table[key].col`
    StaticRead {
        table: String,
        key: Box<Expr>,
        col: String,
    },
    /// Binary operation: `lhs op rhs`
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Unary operation: `op operand`
    UnaryOp { op: UnaryOp, operand: Box<Expr> },
    /// Hash call: `hash(args...)`
    Hash(Vec<Expr>),
    /// Divmod call: `divmod(lhs, rhs)` — only valid in `let (a, b) = ...`
    Divmod { lhs: Box<Expr>, rhs: Box<Expr> },
    /// Select call: `select(cond, if_true, if_false)`
    Select {
        cond: Box<Expr>,
        if_true: Box<Expr>,
        if_false: Box<Expr>,
    },
}

/// Binary operators.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    // Logical
    And,
    Or,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical not: `!`
    Not,
    /// Arithmetic negation: `-`
    Neg,
}
