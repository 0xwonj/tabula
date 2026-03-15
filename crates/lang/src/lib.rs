//! Tabula DSL compiler.
//!
//! Compiles `.tab` source files into Tabula IR (`Vec<TableSchema>` + `Vec<TxTypeDef>`).

pub mod ast;
pub mod error;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod span;
pub mod token;

use error::CompileError;
use lower::LoweredProgram;

/// Compile Tabula DSL source into IR.
///
/// This is the main entry point. It runs the full pipeline:
/// lex → parse → lower → IR.
pub fn compile(source: &str) -> Result<LoweredProgram, Vec<CompileError>> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(tokens)?;
    lower::lower(&ast)
}
