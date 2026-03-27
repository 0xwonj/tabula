//! Canonical runtime harness helpers built on public runtime seams.

use tabula_runtime::{ProveResult, TabulaRuntime, VerifiedResult};

use crate::exec::register_program_from_source;

/// Shared name for the runtime proof output.
pub type ProvedExecution = ProveResult;

/// Shared name for prove-and-verify output.
pub type VerifiedExecution = VerifiedResult;

/// Build the runtime from one registered program.
pub fn build_runtime(registered: tabula_compiler::RegisteredProgram) -> TabulaRuntime {
    TabulaRuntime::builder(registered)
        .expect("create runtime builder")
        .build()
        .expect("build runtime")
}

/// Build the runtime directly from rewritten source.
pub fn runtime_from_source(source: &str) -> TabulaRuntime {
    build_runtime(register_program_from_source(source))
}
