//! Canonical metadata envelope used for proof compatibility checks.

use serde::{Deserialize, Serialize};

const METADATA_MAGIC: [u8; 4] = *b"TCME";
const METADATA_SERIALIZATION_VERSION: u8 = 2;
const METADATA_HASH_DOMAIN: &[u8] = b"tabula.contract_metadata_envelope.v2";

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
    /// Binding registry version.
    pub binding_registry_version: u32,
    /// Execution statement schema version.
    pub statement_schema_version: u32,
    /// Verifier profile version.
    pub verifier_profile_version: u32,
    /// Optional semantic hash stub (reserved for staged rollout).
    pub semantic_hash_stub: Option<[u8; 32]>,
}

impl ContractMetadataEnvelope {
    /// Serialize to canonical binary format.
    ///
    /// Encoding:
    /// 1. magic (`TCME`, 4 bytes)
    /// 2. serialization version (u8)
    /// 3. `profile_hash` (32 bytes)
    /// 4. `contract_schema_version` (u32 big-endian)
    /// 5. `binding_registry_version` (u32 big-endian)
    /// 6. `statement_schema_version` (u32 big-endian)
    /// 7. `verifier_profile_version` (u32 big-endian)
    /// 8. semantic flag (u8; 0/1)
    /// 9. `semantic_hash_stub` (32 bytes, only if flag=1)
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(&METADATA_MAGIC);
        out.push(METADATA_SERIALIZATION_VERSION);
        out.extend_from_slice(&self.profile_hash);
        out.extend_from_slice(&self.contract_schema_version.to_be_bytes());
        out.extend_from_slice(&self.binding_registry_version.to_be_bytes());
        out.extend_from_slice(&self.statement_schema_version.to_be_bytes());
        out.extend_from_slice(&self.verifier_profile_version.to_be_bytes());
        match self.semantic_hash_stub {
            Some(hash) => {
                out.push(1);
                out.extend_from_slice(&hash);
            }
            None => out.push(0),
        }
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
