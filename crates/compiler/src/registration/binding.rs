use blake3::Hasher;
use sha2::Digest as _;

use tabula_contract::{ContractMetadataEnvelope, ProgramBinding};
use tabula_core::TableSchema;
use tabula_ir as ir;
use tabula_profile::ProfileCatalog;

use crate::pipeline::StateFieldSchemeBinding;

pub(crate) fn compute_profile_hash(
    table_schemas: &[TableSchema],
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    let mut tables = table_schemas.to_vec();
    tables.sort_by_key(|schema| schema.id);
    for schema in &mut tables {
        schema.columns.sort_by_key(|column| column.id);
    }

    let mut hasher = Hasher::new();
    hasher.update(b"tabula.driver.profile_hash.v1");
    hasher.update(&(tables.len() as u32).to_be_bytes());
    for schema in &tables {
        hasher.update(&borsh::to_vec(schema)?);
    }
    let profile_catalog_bytes = serde_json::to_vec(profile_catalog)?;
    hasher.update(&(profile_catalog_bytes.len() as u32).to_be_bytes());
    hasher.update(&profile_catalog_bytes);
    Ok(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_semantic_hash_stub(
    program: &ir::Program,
    field_schemes: &[StateFieldSchemeBinding],
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"tabula.driver.semantic_hash.v1");
    hasher.update((field_schemes.len() as u32).to_be_bytes());
    for binding in field_schemes {
        hasher.update(&borsh::to_vec(binding)?);
    }
    hasher.update(&borsh::to_vec(program)?);
    hasher.update(serde_json::to_vec(profile_catalog)?);
    Ok(hasher.finalize().into())
}

pub(crate) fn compute_program_binding(
    program: &ir::Program,
    field_schemes: &[StateFieldSchemeBinding],
    metadata_envelope: &ContractMetadataEnvelope,
) -> anyhow::Result<ProgramBinding> {
    let mut hasher = Hasher::new();
    hasher.update(b"tabula.contract.program_binding.v1");
    hasher.update(&borsh::to_vec(program)?);
    hasher.update(&(field_schemes.len() as u32).to_be_bytes());
    for binding in field_schemes {
        hasher.update(&borsh::to_vec(binding)?);
    }
    let program_hash = hasher.finalize().to_hex().to_string();
    Ok(ProgramBinding::new(
        program_hash,
        metadata_envelope.canonical_hash_hex(),
    ))
}
