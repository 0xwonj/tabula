//! Compiler: canonical semantic ownership for rewritten program compilation,
//! registration, and contract metadata generation.

mod catalogs;
mod error;
mod hir_lower;
mod mir;
mod pipeline;
mod registration;

pub use catalogs::{CompilerCatalogs, SourceCapabilityCatalog, SourceCapabilityDescriptor};
pub use error::{
    CompileDiagnostic, CompileStage, CompilerCatalogError, CompilerError, CompilerResult,
};
pub use pipeline::{
    CompiledProgram, RegisteredProgram, compile_and_register_program_source,
    compile_program_source, compile_program_source_with_catalogs, load_registered_program,
    parse_registered_program, register_compiled_program,
};
