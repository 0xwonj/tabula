//! Profile and semantic hash computation for program contract identity.

use anyhow::Context;
use serde::Serialize;
use sha2::{Digest as _, Sha256};

use tabula_artifact::PrecompileDescriptor;
use tabula_core::TableSchema;
use tabula_ir::PropertyRequirement;
use tabula_ir::TxTypeDef;
use tabula_profile::ProfileCatalog;

const PROFILE_HASH_DOMAIN: &[u8] = b"tabula.driver.profile_hash.v1";
const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";

pub(crate) fn compute_profile_hash(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    let canonical_schemas = canonicalize_schemas(schemas);
    let canonical_tx_types = canonicalize_tx_types(tx_types);

    let mut hasher = blake3::Hasher::new();
    hasher.update(PROFILE_HASH_DOMAIN);
    hasher.update(&(canonical_schemas.len() as u32).to_be_bytes());
    for schema in &canonical_schemas {
        hasher.update(&borsh::to_vec(schema).context("failed to borsh-encode table schema")?);
    }

    hasher.update(&(canonical_tx_types.len() as u32).to_be_bytes());
    for tx in &canonical_tx_types {
        hasher.update(&borsh::to_vec(tx).context("failed to borsh-encode tx type")?);
    }

    let profile_catalog_bytes =
        serde_json::to_vec(profile_catalog).context("failed to encode profile catalog")?;
    hasher.update(&(profile_catalog_bytes.len() as u32).to_be_bytes());
    hasher.update(&profile_catalog_bytes);

    Ok(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_semantic_hash_stub(
    precompile_manifest: &[PrecompileDescriptor],
    required_property_requirements: &[PropertyRequirement],
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    #[derive(Serialize)]
    struct SemanticContract<'a> {
        precompile_manifest: &'a [PrecompileDescriptor],
        required_property_requirements: &'a [PropertyRequirement],
        profile_catalog: &'a ProfileCatalog,
    }

    let payload = serde_json::to_vec(&SemanticContract {
        precompile_manifest,
        required_property_requirements,
        profile_catalog,
    })
    .context("failed to canonicalize semantic contract")?;

    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_HASH_DOMAIN);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    Ok(hasher.finalize().into())
}

fn canonicalize_schemas(schemas: &[TableSchema]) -> Vec<TableSchema> {
    let mut out = schemas.to_vec();
    out.sort_by_key(|s| s.id);
    for schema in &mut out {
        schema.columns.sort_by_key(|c| c.id);
    }
    out
}

fn canonicalize_tx_types(tx_types: &[TxTypeDef]) -> Vec<TxTypeDef> {
    let mut out = tx_types.to_vec();
    out.sort_by_key(|tx| tx.id);
    out
}
