use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::ProfileError;

const JSON_HASH_DOMAIN: &[u8] = b"tabula.profile.json_hash.v1";

pub(crate) fn canonical_json_digest<T: Serialize>(
    label: &str,
    value: &T,
) -> Result<[u8; 32], ProfileError> {
    let bytes = serde_json::to_vec(value).map_err(ProfileError::EncodeJson)?;
    let mut hasher = Sha256::new();
    hasher.update(JSON_HASH_DOMAIN);
    hasher.update(label.as_bytes());
    hasher.update([0u8]);
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}
