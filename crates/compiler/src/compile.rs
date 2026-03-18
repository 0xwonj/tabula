//! Compilation pipeline for `.tab` source programs.

use crate::ProgramDefinition;
use crate::error::{CompileDiagnostic, CompilerError, CompilerResult};

/// Compile a `.tab` source string into canonical source definitions.
pub fn compile_program_source(source: &str) -> CompilerResult<ProgramDefinition> {
    match tabula_lang::compile(source) {
        Ok(compiled) => Ok(ProgramDefinition {
            table_schemas: compiled.schemas,
            tx_types: compiled.tx_types,
            column_schemes: compiled
                .column_schemes
                .into_iter()
                .map(|selection| crate::sources::ColumnSchemeSelection {
                    table_id: selection.table_id,
                    col_id: selection.col_id,
                    scheme_id: selection.scheme_id,
                })
                .collect(),
        }),
        Err(errors) => Err(CompilerError::Compile {
            diagnostics: compile_diagnostics(source, &errors),
        }),
    }
}

/// Convert lang compile errors into structured diagnostics.
pub(crate) fn compile_diagnostics(
    source: &str,
    errors: &[tabula_lang::error::CompileError],
) -> Vec<CompileDiagnostic> {
    errors
        .iter()
        .map(|err| {
            let (line, col) = tabula_lang::span::line_col(source, err.span.start);
            CompileDiagnostic {
                kind: format!("{:?}", err.kind),
                message: err.message.clone(),
                span_start: err.span.start,
                span_end: err.span.end,
                line,
                col,
            }
        })
        .collect()
}
