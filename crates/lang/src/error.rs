//! Frontend diagnostic error types for parsing, name resolution, and type checking.

use std::fmt;

use thiserror::Error;

use crate::span::Span;

/// A diagnostic error produced by the lang frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendError {
    /// The error category.
    pub kind: FrontendErrorKind,
    /// Source location of the error.
    pub span: Span,
    /// Human-readable error description.
    pub message: String,
}

impl FrontendError {
    /// Create a new frontend error.
    pub fn new(kind: FrontendErrorKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for FrontendError {}

/// Category of a frontend error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FrontendErrorKind {
    /// A character the lexer does not recognize.
    #[error("unexpected character")]
    UnexpectedChar,
    /// A string literal was not closed.
    #[error("unterminated string")]
    UnterminatedString,
    /// A hex literal contained non-hex characters or had the wrong length.
    #[error("invalid hex literal")]
    InvalidHexLiteral,
    /// An integer literal exceeded the representable range.
    #[error("integer overflow")]
    IntegerOverflow,
    /// The parser encountered a token it did not expect.
    #[error("unexpected token")]
    UnexpectedToken,
    /// The parser expected a specific token that was absent.
    #[error("expected token")]
    ExpectedToken,
    /// A name was declared more than once in the same scope.
    #[error("duplicate symbol")]
    DuplicateSymbol,
    /// A referenced name was not defined.
    #[error("undefined symbol")]
    UndefinedSymbol,
    /// A type name could not be resolved to a known type.
    #[error("type resolution failed")]
    TypeResolution,
    /// An expression's type did not match the expected type.
    #[error("type mismatch")]
    TypeMismatch,
    /// A constant expression contained a non-constant sub-expression.
    #[error("invalid const expression")]
    InvalidConstExpr,
    /// A language feature is not yet supported.
    #[error("unsupported feature")]
    UnsupportedFeature,
    /// The program is structurally invalid (e.g. missing required declarations).
    #[error("invalid program")]
    InvalidProgram,
}
