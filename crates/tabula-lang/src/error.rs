//! Compile error types with source location tracking.

use std::fmt;

use crate::span::{Span, line_col};

/// A compile error with source location and human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// What kind of error.
    pub kind: ErrorKind,
    /// Where in the source the error occurred.
    pub span: Span,
    /// Human-readable error message.
    pub message: String,
}

impl CompileError {
    /// Create a new compile error.
    pub fn new(kind: ErrorKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    /// Format the error with source context for display.
    pub fn display_with_source<'a>(&'a self, source: &'a str) -> ErrorDisplay<'a> {
        ErrorDisplay {
            error: self,
            source,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CompileError {}

/// Helper for displaying errors with source context.
pub struct ErrorDisplay<'a> {
    error: &'a CompileError,
    source: &'a str,
}

impl fmt::Display for ErrorDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (line, col) = line_col(self.source, self.error.span.start);
        write!(
            f,
            "error[{:?}] at {}:{}: {}",
            self.error.kind, line, col, self.error.message
        )
    }
}

/// Classification of compile errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // --- Lexer errors ---
    /// Unexpected character in input.
    UnexpectedChar,
    /// String literal missing closing quote.
    UnterminatedString,
    /// Invalid hex literal (wrong length, bad chars).
    InvalidHexLiteral,
    /// Integer literal too large for u64.
    IntegerOverflow,

    // --- Parser errors ---
    /// Got an unexpected token.
    UnexpectedToken,
    /// Expected a specific token that wasn't found.
    ExpectedToken,

    // --- Resolution errors ---
    /// Reference to an undefined table.
    UndefinedTable,
    /// Reference to an undefined column.
    UndefinedColumn,
    /// Reference to an undefined variable.
    UndefinedVariable,
    /// Duplicate `let` binding name within a tx.
    DuplicateBinding,
    /// Duplicate table declaration.
    DuplicateTable,
    /// Duplicate tx declaration.
    DuplicateTx,
    /// Duplicate column in a table.
    DuplicateColumn,
    /// Duplicate parameter name in a tx.
    DuplicateParam,

    // --- Type errors ---
    /// Operand types don't match or are incompatible.
    TypeMismatch,
}
