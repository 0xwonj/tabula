//! Profile hash computation for program contract identity.

use anyhow::Context;

use tabula_core::TableSchema;
use tabula_ir::TxTypeDef;

const PROFILE_HASH_DOMAIN: &[u8] = b"tabula.driver.profile_hash.v1";

pub(crate) fn compute_profile_hash(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
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

    Ok(*hasher.finalize().as_bytes())
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
