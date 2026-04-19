//! Canonical runtime harness helpers built on public runtime seams.

use std::sync::Arc;

use tabula_contract::SealedArtifact;
use tabula_runtime::{
    PreparedExecutor, PreparedOptions, PreparedProver, PreparedVerifier, ProofOutcome,
    prepare_executor, prepare_prover, prepare_verifier,
};

use crate::exec::register_program_from_source;

/// Shared name for the runtime proof output (prove or prove-and-verify).
pub type ProvedExecution = ProofOutcome;

/// Shared name for prove-and-verify output.
pub type VerifiedExecution = ProofOutcome;

/// Build a [`PreparedExecutor`] from one registered program.
pub fn build_executor(registered: tabula_compiler::RegisteredProgram) -> PreparedExecutor {
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    prepare_executor(Arc::new(registered), &opts).expect("build prepared executor")
}

/// Build a [`PreparedExecutor`] directly from rewritten source.
pub fn executor_from_source(source: &str) -> PreparedExecutor {
    build_executor(register_program_from_source(source))
}

/// Build a [`PreparedProver`] from one registered program.
pub fn build_prover(registered: tabula_compiler::RegisteredProgram) -> PreparedProver {
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    prepare_prover(Arc::new(registered), &opts).expect("build prepared prover")
}

/// Build a [`PreparedVerifier`] from a sealed artifact.
pub fn build_verifier(sealed: Arc<SealedArtifact>) -> PreparedVerifier {
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    prepare_verifier(sealed, &opts).expect("build prepared verifier")
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
