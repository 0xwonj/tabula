//! Canonical runtime harness helpers built on public runtime seams.

use std::sync::Arc;

use tabula_contract::SealedArtifact;
use tabula_runtime::{
    PreparedProver, PreparedVerifier, ProveResult, TabulaRuntime, VerifiedResult,
};

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

/// Build a [`PreparedProver`] from one registered program.
pub fn build_prover(registered: tabula_compiler::RegisteredProgram) -> PreparedProver {
    PreparedProver::builder(registered)
        .expect("create prover builder")
        .build()
        .expect("build prepared prover")
}

/// Build a [`PreparedVerifier`] from a sealed artifact.
pub fn build_verifier(sealed: Arc<SealedArtifact>) -> PreparedVerifier {
    PreparedVerifier::builder(sealed)
        .expect("create verifier builder")
        .build()
        .expect("build prepared verifier")
}

/// Build a [`PreparedVerifier`] from a registered program (extracts the sealed artifact).
pub fn build_verifier_from_registered(
    registered: &tabula_compiler::RegisteredProgram,
) -> PreparedVerifier {
    build_verifier(Arc::new(registered.sealed().clone()))
}

/// Build an `Arc<SealedArtifact>` from rewritten Tabula source.
pub fn sealed_artifact_from_source(src: &str) -> Arc<SealedArtifact> {
    Arc::new(register_program_from_source(src).sealed().clone())
}
