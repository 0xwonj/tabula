//! Canonical runtime harness helpers built on public runtime seams.

use tabula_core::mock::Blake3Hasher;
use tabula_runtime::{
    ExecutedBatch, ProveInput, ProveResult, RuntimeError, TabulaRuntime, VerifiedResult, Verifier,
};

use crate::exec::compiled_program_from_artifact;
use crate::fixtures::cases::{ArtifactRuntimeCase, CompiledRuntimeCase};

/// Shared name for the runtime proof output.
pub type ProvedExecution = ProveResult;

/// Shared name for prove-and-verify output.
pub type VerifiedExecution = VerifiedResult;

/// Build a runtime from one compiled program using only public runtime seams.
pub fn build_runtime(compiled: tabula_compiler::SealedProgram) -> TabulaRuntime {
    TabulaRuntime::builder(compiled)
        .build()
        .expect("build runtime")
}

/// Execute one compiled runtime case with the canonical runtime harness.
pub fn execute_compiled_case(case: &CompiledRuntimeCase) -> ExecutedBatch {
    build_runtime(case.compiled_program.clone())
        .execute(&case.state, &case.batch)
        .expect("execute compiled runtime case")
}

/// Execute one artifact runtime case by first compiling its sealed artifact.
pub fn execute_artifact_case(case: &ArtifactRuntimeCase) -> ExecutedBatch {
    build_runtime(compiled_program_from_artifact(&case.artifact))
        .execute(&case.state, &case.batch)
        .expect("execute artifact runtime case")
}

/// Prove one compiled runtime case with the canonical runtime harness.
pub fn prove_compiled_case(case: &CompiledRuntimeCase) -> ProvedExecution {
    let runtime = build_runtime(case.compiled_program.clone());
    let executed = runtime
        .execute(&case.state, &case.batch)
        .expect("execute compiled runtime case");
    runtime
        .prove(&ProveInput {
            state: &case.state,
            batch: &case.batch,
            executed: &executed,
        })
        .expect("prove compiled runtime case")
}

/// Prove and verify one artifact runtime case with the canonical runtime harness.
pub fn prove_and_verify_artifact_case(case: &ArtifactRuntimeCase) -> VerifiedExecution {
    let runtime = build_runtime(compiled_program_from_artifact(&case.artifact));
    let executed = runtime
        .execute(&case.state, &case.batch)
        .expect("execute artifact runtime case");
    runtime
        .prove_and_verify(&ProveInput {
            state: &case.state,
            batch: &case.batch,
            executed: &executed,
        })
        .expect("prove and verify artifact runtime case")
}

/// Verify a proved execution against the artifact-bound verifier seam.
pub fn verify_artifact_case(
    case: &ArtifactRuntimeCase,
    proved: &ProvedExecution,
) -> Result<(), RuntimeError> {
    let verifier = Verifier::builder(case.artifact.clone()).build()?;
    verifier.verify(&proved.proof, &proved.statement)
}

/// Execute one compiled case through the free execution seam.
pub fn execute_compiled_case_free(
    case: &CompiledRuntimeCase,
) -> Result<ExecutedBatch, RuntimeError> {
    tabula_runtime::run_compiled_batch(&tabula_runtime::CompiledBatchInput {
        compiled_program: &case.compiled_program,
        state: &case.state,
        batch: &case.batch,
        hasher: &Blake3Hasher,
    })
}
