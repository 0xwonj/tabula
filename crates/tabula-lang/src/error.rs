//! Compile error types with source location tracking.

use std::fmt;

use crate::span::{Span, line_col, source_line};

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
        let (line_num, line_text) = source_line(self.source, self.error.span.start);
        let gutter_width = line_num.to_string().len();

        // Header: error[Kind]: message
        writeln!(f, "error[{:?}]: {}", self.error.kind, self.error.message)?;

        // Location: --> line:col
        writeln!(f, "{:>width$}--> {}:{}", "", line, col, width = gutter_width)?;

        // Gutter separator
        writeln!(f, "{:>width$} |", "", width = gutter_width)?;

        // Source line
        writeln!(f, "{} | {}", line_num, line_text)?;

        // Caret underline
        // col is 1-indexed; compute underline width (clamp to current line)
        let line_start_offset = self.error.span.start - (col - 1);
        let line_end_offset = line_start_offset + line_text.len();
        let span_end_on_line = self.error.span.end.min(line_end_offset);
        let underline_width = span_end_on_line.saturating_sub(self.error.span.start).max(1);

        write!(
            f,
            "{:>width$} | {:>pad$}{} {}",
            "",
            "",
            "^".repeat(underline_width),
            self.error.message,
            width = gutter_width,
            pad = col - 1,
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
