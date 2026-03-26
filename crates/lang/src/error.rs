use std::fmt;

use thiserror::Error;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendError {
    pub kind: FrontendErrorKind,
    pub span: Span,
    pub message: String,
}

impl FrontendError {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FrontendErrorKind {
    #[error("unexpected character")]
    UnexpectedChar,
    #[error("unterminated string")]
    UnterminatedString,
    #[error("invalid hex literal")]
    InvalidHexLiteral,
    #[error("integer overflow")]
    IntegerOverflow,
    #[error("unexpected token")]
    UnexpectedToken,
    #[error("expected token")]
    ExpectedToken,
    #[error("duplicate symbol")]
    DuplicateSymbol,
    #[error("undefined symbol")]
    UndefinedSymbol,
    #[error("type resolution failed")]
    TypeResolution,
    #[error("type mismatch")]
    TypeMismatch,
    #[error("invalid const expression")]
    InvalidConstExpr,
    #[error("unsupported feature")]
    UnsupportedFeature,
    #[error("invalid program")]
    InvalidProgram,
}
