//! Verifier-facing statement binding contract owned by `tabula-contract`.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_commitment::PoseidonHasher;
use tabula_core::traits::Hasher;
use tabula_core::{Digest, ProgramId};

use crate::PublicStatement;
use crate::binding::ProgramBinding;
use crate::error::ProofContractError;
use crate::versions::{STATEMENT_SCHEMA_VERSION, validate_statement_schema_version};

const BOUND_STATEMENT_DOMAIN: &[u8] = b"tabula.contract.artifact_bound_statement.v1";

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CanonicalBoundStatement {
    schema_version: u32,
    program_hash: Digest,
    metadata_hash: Digest,
    program_id: ProgramId,
    static_table_root: Digest,
    old_state_root: Digest,
    new_state_root: Digest,
    public_context_digest: Digest,
    applied_tx_digest: Digest,
    event_digest: Digest,
}

/// Artifact-derived binding input recomputed from the sealed artifact by the verifier.
///
/// This is only the artifact side of verification. It does not include prepared
/// proof-system verifier state such as the machine setup or relation policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ArtifactContext {
    /// Compiler-sealed program binding.
    pub binding: ProgramBinding,
    /// Program identifier sealed by the artifact.
    pub program_id: ProgramId,
    /// Transcript-bound root of the sealed static relation table set.
    pub static_table_root: Digest,
}

impl ArtifactContext {
    /// Build one artifact-derived verifier context.
    #[must_use]
    pub const fn new(
        binding: ProgramBinding,
        program_id: ProgramId,
        static_table_root: Digest,
    ) -> Self {
        Self {
            binding,
            program_id,
            static_table_root,
        }
    }
}

/// Transcript-bound verifier object derived from the sealed artifact and one proved public statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundStatement {
    /// Canonical proof statement schema version.
    pub schema_version: u32,
    /// Artifact-derived verifier context.
    pub context: ArtifactContext,
    /// AIR-proved public statement.
    pub public_statement: PublicStatement,
}

impl BoundStatement {
    /// Construct one bound statement using the current contract schema version.
    #[must_use]
    pub fn new(context: ArtifactContext, public_statement: PublicStatement) -> Self {
        Self {
            schema_version: STATEMENT_SCHEMA_VERSION,
            context,
            public_statement,
        }
    }

    /// Borrow the artifact-derived verifier context.
    #[must_use]
    pub const fn context(&self) -> &ArtifactContext {
        &self.context
    }

    /// Borrow the AIR-proved public statement.
    #[must_use]
    pub const fn public_statement(&self) -> &PublicStatement {
        &self.public_statement
    }

    /// Serialize the statement canonically for transcript binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProofContractError> {
        validate_statement_schema_version(self.schema_version)?;

        let canonical = CanonicalBoundStatement {
            schema_version: self.schema_version,
            program_hash: *self.context.binding.program_hash(),
            metadata_hash: *self.context.binding.metadata_hash(),
            program_id: self.context.program_id,
            static_table_root: self.context.static_table_root,
            old_state_root: self.public_statement.old_root.to_bytes(),
            new_state_root: self.public_statement.new_root.to_bytes(),
            public_context_digest: self.public_statement.public_context_digest.to_bytes(),
            applied_tx_digest: self.public_statement.applied_tx_digest.to_bytes(),
            event_digest: self.public_statement.event_digest.to_bytes(),
        };

        let mut bytes = BOUND_STATEMENT_DOMAIN.to_vec();
        bytes.extend(borsh::to_vec(&canonical).map_err(|error| {
            ProofContractError::StatementEncode {
                detail: error.to_string(),
            }
        })?);
        Ok(bytes)
    }

    /// Canonical transcript-bound digest of the artifact-bound public statement.
    pub fn binding_digest(&self) -> Result<[u8; 32], ProofContractError> {
        Ok(PoseidonHasher::new().hash(&self.canonical_bytes()?))
    }
}
