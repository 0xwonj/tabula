//! Compilation pipeline for `.tab` source programs.

use tabula_artifact::ProgramArtifact;

use crate::ProgramSourceFile;
use crate::error::{CompileDiagnostic, DriverError, DriverResult};

/// Compile a `.tab` source string into a program artifact source file.
pub fn compile_program_source(source: &str) -> DriverResult<ProgramSourceFile> {
    match tabula_lang::compile(source) {
        Ok(compiled) => Ok(ProgramArtifact {
            table_schemas: compiled.schemas,
            tx_types: compiled.tx_types,
            contract_metadata: None,
        }),
        Err(errors) => Err(DriverError::Compile {
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
