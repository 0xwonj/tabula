//! Compiler/runtime metadata compatibility envelope.

use serde::{Deserialize, Serialize};

const METADATA_MAGIC: [u8; 4] = *b"TCME";
const METADATA_SERIALIZATION_VERSION: u8 = 1;
const METADATA_HASH_DOMAIN: &[u8] = b"tabula.contract_metadata_envelope.v1";

/// Canonical metadata envelope used for proof compatibility checks.
///
/// Field order, binary encoding, and hashing are fixed by
/// [`ContractMetadataEnvelope::to_canonical_bytes`] and
/// [`ContractMetadataEnvelope::canonical_hash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractMetadataEnvelope {
    /// Compiler/runtime profile fingerprint.
    pub profile_hash: [u8; 32],
    /// Contract schema version.
    pub contract_schema_version: u32,
    /// Execution statement schema version.
    pub statement_schema_version: u32,
    /// Verifier profile version.
    pub verifier_profile_version: u32,
    /// Compiler/runtime semantic fingerprint.
    pub semantic_hash: [u8; 32],
}

impl ContractMetadataEnvelope {
    /// Serialize to canonical binary format.
    ///
    /// Encoding:
    /// 1. magic (`TCME`, 4 bytes)
    /// 2. serialization version (u8)
    /// 3. `profile_hash` (32 bytes)
    /// 4. `contract_schema_version` (u32 big-endian)
    /// 5. `statement_schema_version` (u32 big-endian)
    /// 6. `verifier_profile_version` (u32 big-endian)
    /// 7. `semantic_hash` (32 bytes)
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(81);
        out.extend_from_slice(&METADATA_MAGIC);
        out.push(METADATA_SERIALIZATION_VERSION);
        out.extend_from_slice(&self.profile_hash);
        out.extend_from_slice(&self.contract_schema_version.to_be_bytes());
        out.extend_from_slice(&self.statement_schema_version.to_be_bytes());
        out.extend_from_slice(&self.verifier_profile_version.to_be_bytes());
        out.extend_from_slice(&self.semantic_hash);
        out
    }

    /// Hash the canonical bytes with fixed domain separation.
    pub fn canonical_hash_bytes(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(METADATA_HASH_DOMAIN);
        hasher.update(&self.to_canonical_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Hash the canonical bytes with fixed domain separation.
    pub fn canonical_hash(&self) -> [u8; 32] {
        self.canonical_hash_bytes()
    }

    /// Hash the canonical bytes and return the digest as lowercase hex.
    pub fn canonical_hash_hex(&self) -> String {
        let hash = self.canonical_hash_bytes();
        let mut out = String::with_capacity(hash.len() * 2);
        for byte in hash {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}
