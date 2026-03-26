use tabula_core::error::TabulaError;
use tabula_lang::FrontendError;
use tabula_lang::span;

use crate::error::{CompileDiagnostic, CompileStage, CompilerError};

pub(crate) fn compile_error(diagnostics: Vec<CompileDiagnostic>) -> CompilerError {
    CompilerError::Compile { diagnostics }
}

pub(crate) fn diagnostic_from_compiler_error(
    source: &str,
    stage: CompileStage,
    error: CompilerError,
) -> CompilerError {
    match error {
        CompilerError::Compile { diagnostics } => CompilerError::Compile { diagnostics },
        CompilerError::InvalidProgram(source_error) => compile_error(vec![spanless_diagnostic(
            stage,
            source,
            "InvalidProgram",
            source_error.to_string(),
        )]),
        other => other,
    }
}

pub(crate) fn frontend_diagnostic(
    source: &str,
    stage: CompileStage,
    error: FrontendError,
) -> CompileDiagnostic {
    let (line, col) = span::line_col(source, error.span.start);
    CompileDiagnostic {
        stage,
        kind: format!("{:?}", error.kind),
        message: error.message,
        span_start: error.span.start,
        span_end: error.span.end,
        line,
        col,
    }
}

pub(crate) fn tabula_diagnostic(
    stage: CompileStage,
    source: &str,
    error: &TabulaError,
) -> CompileDiagnostic {
    spanless_diagnostic(stage, source, "InvalidProgram", error.to_string())
}

pub(crate) fn spanless_diagnostic(
    stage: CompileStage,
    source: &str,
    kind: &str,
    message: String,
) -> CompileDiagnostic {
    let (line, col) = span::line_col(source, 0);
    CompileDiagnostic {
        stage,
        kind: kind.to_string(),
        message,
        span_start: 0,
        span_end: 0,
        line,
        col,
    }
}
