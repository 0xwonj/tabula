//! Relation policy sealed at program registration time.
//!
//! `SealedRelationPolicy` is computed once from the IR during compilation and
//! stored as part of the sealed artifact. Runtime consumers read this value
//! instead of re-scanning the IR at prove/verify time.

use serde::{Deserialize, Serialize};

/// Relation-table policy derived from the program's IR at registration time.
///
/// The enum is sealed: its value is fixed when the program is registered
/// and cannot change without re-registration. Runtime code reads this field
/// from the `SealedArtifact` (or `RegisteredProgram`) rather than scanning
/// IR opcodes at prepare time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SealedRelationPolicy {
    /// No relation tables are used; the artifact root is not checked.
    Disabled,
    /// Relation tables are used; the artifact root must match.
    RequireArtifactRoot,
}

impl SealedRelationPolicy {
    /// Returns `true` when the policy requires the artifact relation-table root.
    pub const fn requires_artifact_root(self) -> bool {
        matches!(self, Self::RequireArtifactRoot)
    }
}
