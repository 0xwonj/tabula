use tabula_artifact::{ExecutionStatement, ProgramArtifact};
use tabula_machine::TabulaProof;
use tabula_runtime::ProgramVerifier;

/// Assert that a statement is bound to the expected sealed program artifact.
pub fn assert_statement_matches_artifact(
    statement: &ExecutionStatement,
    artifact: &ProgramArtifact,
) {
    let expected_program_hash = artifact
        .canonical_digest()
        .expect("compute expected program digest");
    let expected_metadata_hash = artifact.contract_metadata.canonical_hash_hex();
    assert_eq!(
        statement.program_hash, expected_program_hash,
        "statement program hash differs from artifact digest"
    );
    assert_eq!(
        statement.metadata_hash, expected_metadata_hash,
        "statement metadata hash differs from artifact metadata"
    );
}

/// Assert that a verifier accepts the given proof and statement.
pub fn assert_proof_verifies(
    verifier: &ProgramVerifier,
    proof: &TabulaProof,
    statement: &ExecutionStatement,
) {
    verifier
        .verify(proof, statement)
        .expect("proof should verify against statement");
}
