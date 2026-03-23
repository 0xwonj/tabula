//! Tabula DSL compiler.
//!
//! Compiles `.tab` source files into source-side semantic schemas plus Tabula IR.

use std::collections::BTreeMap;

pub mod ast;
pub mod error;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod span;
pub mod token;

use error::CompileError;
use lower::LoweredProgram;
use tabula_ir::{PrecompileId, PrecompileSignature};

/// Compile Tabula DSL source into IR.
///
/// This is the main entry point. It runs the full pipeline:
/// lex → parse → lower → IR.
pub fn compile(source: &str) -> Result<LoweredProgram, Vec<CompileError>> {
    compile_with_precompiles(source, &BTreeMap::new())
}

/// Compile Tabula DSL source into IR using explicit precompile signatures.
pub fn compile_with_precompiles(
    source: &str,
    precompiles: &BTreeMap<PrecompileId, PrecompileSignature>,
) -> Result<LoweredProgram, Vec<CompileError>> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(tokens)?;
    lower::lower_with_registry_and_precompiles(
        &ast,
        &tabula_profile::builtin_semantic_registry()
            .expect("built-in semantic registry must stay valid"),
        precompiles,
    )
}
