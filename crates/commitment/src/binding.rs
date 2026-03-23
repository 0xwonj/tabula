//! Canonical column root-binding contracts exported by the commitment crate.

use tabula_core::{ColId, Digest, RootProfileId, TableId};

use crate::primitives::NativeDigest;

/// Canonical normalized verifier-visible digest exported by one column backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NormalizedVerifierDigest {
    /// Native field-element digest bound into root commitments and proof identity.
    pub digest: NativeDigest,
}

impl NormalizedVerifierDigest {
    /// Wrap one native digest as a normalized verifier-visible digest.
    pub const fn new(digest: NativeDigest) -> Self {
        Self { digest }
    }
}

/// Canonical root-binding contract for one column transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnRootBinding {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Root-binding family used to serialize the leaf contract.
    pub root_binding_family: RootProfileId,
    /// Sealed column profile hash bound into the leaf prefix.
    pub column_profile_hash: Digest,
    /// Concrete digest prefix used by the leaf contract.
    pub binding_digest: NativeDigest,
    /// Verifier-visible digest before the batch.
    pub old_digest: NormalizedVerifierDigest,
    /// Verifier-visible digest after the batch.
    pub new_digest: NormalizedVerifierDigest,
    /// Column was empty before the batch.
    pub is_empty_old: bool,
    /// Column is empty after the batch.
    pub is_empty_new: bool,
    /// Column was modified in this batch.
    pub is_touched: bool,
}
