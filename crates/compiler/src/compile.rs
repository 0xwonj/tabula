//! Compilation pipeline for `.tab` source programs.

use std::collections::BTreeMap;

use tabula_ir::{PrecompileId, PrecompileSignature};

use crate::CompilerCatalogs;
use crate::ProgramDefinition;
use crate::error::{CompileDiagnostic, CompilerError, CompilerResult};

/// Compile a `.tab` source string into canonical source definitions.
pub fn compile_program_source(source: &str) -> CompilerResult<ProgramDefinition> {
    compile_program_source_with_precompiles(source, &BTreeMap::new())
}

/// Compile a `.tab` source string using one explicit compiler catalog set.
pub fn compile_program_source_with_catalogs(
    source: &str,
    catalogs: &CompilerCatalogs,
) -> CompilerResult<ProgramDefinition> {
    let precompiles = catalogs
        .precompile_descriptors()
        .iter()
        .map(|(id, descriptor)| (*id, descriptor.signature.clone()))
        .collect::<BTreeMap<PrecompileId, PrecompileSignature>>();
    compile_program_source_with_precompiles(source, &precompiles)
}

fn compile_program_source_with_precompiles(
    source: &str,
    precompiles: &BTreeMap<PrecompileId, PrecompileSignature>,
) -> CompilerResult<ProgramDefinition> {
    match tabula_lang::compile_with_precompiles(source, precompiles) {
        Ok(compiled) => Ok(ProgramDefinition {
            table_schemas: compiled
                .schemas
                .into_iter()
                .map(|schema| crate::sources::SourceTableSchema {
                    id: schema.id,
                    name: schema.name,
                    columns: schema
                        .columns
                        .into_iter()
                        .map(|column| crate::sources::SourceColumnDef {
                            id: column.id,
                            name: column.name,
                            type_id: column.type_id,
                        })
                        .collect(),
                })
                .collect(),
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
