use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ArtifactError;

const JSON_HASH_DOMAIN: &[u8] = b"tabula.artifact.json_hash.v1";

pub(crate) fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ArtifactError> {
    serde_json::to_vec(value).map_err(ArtifactError::EncodeJson)
}

pub(crate) fn canonical_json_digest<T: Serialize>(
    label: &str,
    value: &T,
) -> Result<[u8; 32], ArtifactError> {
    let bytes = canonical_json_bytes(value)?;
    let mut hasher = Sha256::new();
    hasher.update(JSON_HASH_DOMAIN);
    hasher.update(label.as_bytes());
    hasher.update([0u8]);
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}

pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}
