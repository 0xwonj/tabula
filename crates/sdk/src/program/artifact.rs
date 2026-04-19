use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tabula_compiler::RegisteredProgram;

use super::schema::Schema;
use crate::error::SdkError;

#[derive(Debug)]
struct ArtifactInner {
    registered: RegisteredProgram,
    digest: String,
    schema: Schema,
}

/// Sealed portable program artifact.
#[derive(Debug, Clone)]
pub struct Artifact {
    inner: Arc<ArtifactInner>,
}

impl Artifact {
    pub(crate) fn from_registered(registered: RegisteredProgram) -> Result<Self, SdkError> {
        registered.validate_sealed_artifact()?;
        let digest = registered.canonical_digest()?;
        let schema = Schema::from_registered(&registered)?;
        Ok(Self {
            inner: Arc::new(ArtifactInner {
                registered,
                digest,
                schema,
            }),
        })
    }

    pub(crate) fn registered(&self) -> &RegisteredProgram {
        &self.inner.registered
    }

    /// Borrow the contract-layer sealed artifact.
    pub fn sealed_artifact(&self) -> &tabula_contract::SealedArtifact {
        self.inner.registered.sealed()
    }

    /// SHA-256 hex digest of the canonical serialized artifact.
    pub fn digest(&self) -> &str {
        &self.inner.digest
    }

    /// The program schema (tables, transactions, queries, context fields).
    pub fn schema(&self) -> &Schema {
        &self.inner.schema
    }
}

impl Serialize for Artifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.registered.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Artifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let registered = RegisteredProgram::deserialize(deserializer)?;
        Self::from_registered(registered).map_err(serde::de::Error::custom)
    }
}
