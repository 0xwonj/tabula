use borsh::to_vec as borsh_to_vec;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tabula_artifact::Artifact;
use tabula_core::{ColumnLayoutKind, RootProfileId, SchemeId, SchemeProfileId};
use tabula_ext::{ColumnBackendFactory, ColumnBackendSetup, ExtError, MaterializedColumnBackend};
use tabula_ir::PropertyQueryKind;
use tabula_profile::{
    CanonicalNullEncoding, CommitmentContractKind, EncodingClass, EncodingRequirements,
    FieldFamily, ProfileCatalog, SCHEME_PROFILE_SMT_ID, SCHEME_PROFILE_SSMC_ID, SchemeProfile,
    TranscriptSerialization, VerifierDigestFormat, WidthConstraint, builtin_smt_scheme_profile,
    builtin_ssmc_scheme_profile,
};

use crate::{SmtScheme, SsmcScheme};

const SEMANTIC_HASH_DOMAIN: &[u8] = b"tabula.driver.semantic_hash.v1";
const PROFILE_HASH_DOMAIN: &[u8] = b"tabula.driver.profile_hash.v1";

pub(crate) fn custom_scheme_profile(scheme_id: SchemeId) -> SchemeProfile {
    scheme_profile(
        SchemeProfileId(0x8000_0000 | u32::from(scheme_id.raw())),
        scheme_id,
        ColumnLayoutKind::SSMC_V1,
        RootProfileId::SMT_V1,
        vec![],
    )
}

pub(crate) fn custom_smt_scheme_profile(scheme_id: SchemeId) -> SchemeProfile {
    scheme_profile(
        SchemeProfileId(0x8000_0000 | u32::from(scheme_id.raw())),
        scheme_id,
        ColumnLayoutKind::SMT_V1,
        RootProfileId::SMT_V1,
        vec![],
    )
}

pub(crate) fn unsupported_layout_scheme_profile(scheme_id: SchemeId) -> SchemeProfile {
    scheme_profile(
        SchemeProfileId(0x8000_0000 | u32::from(scheme_id.raw())),
        scheme_id,
        ColumnLayoutKind(0x9000),
        RootProfileId::SMT_V1,
        vec![],
    )
}

pub(crate) fn set_artifact_column_scheme(
    artifact: &mut Artifact,
    index: usize,
    scheme_profile: SchemeProfile,
) {
    let column_profile_id = artifact
        .table_schemas
        .iter()
        .flat_map(|schema| schema.columns.iter())
        .nth(index)
        .expect("column index")
        .column_profile_id;
    let scheme_profile_id =
        replace_or_register_scheme_profile(&mut artifact.profile_catalog, scheme_profile);
    let column_index = artifact
        .profile_catalog
        .columns
        .iter()
        .position(|profile| profile.column_profile_id == column_profile_id)
        .expect("column profile");
    let type_id = artifact.profile_catalog.columns[column_index].type_id;
    let encoding_profile_id = artifact.profile_catalog.columns[column_index].encoding_profile_id;
    let type_descriptor = artifact
        .profile_catalog
        .type_descriptor(type_id)
        .expect("column type descriptor")
        .clone();
    let encoding_profile = artifact
        .profile_catalog
        .encoding_profile(encoding_profile_id)
        .expect("column encoding profile")
        .clone();
    let scheme_profile = artifact
        .profile_catalog
        .scheme_profile(scheme_profile_id)
        .expect("column scheme profile")
        .clone();

    artifact.profile_catalog.columns[column_index].scheme_profile_id = scheme_profile_id;
    artifact.profile_catalog.columns[column_index].profile_hash = artifact.profile_catalog.columns
        [column_index]
        .compute_profile_hash(&type_descriptor, &encoding_profile, &scheme_profile)
        .expect("column profile hash");

    artifact.contract_metadata.profile_hash = compute_profile_hash(
        &artifact.table_schemas,
        &artifact.tx_types,
        &artifact.profile_catalog,
    )
    .expect("profile hash");
    artifact.contract_metadata.semantic_hash_stub = Some(
        compute_semantic_hash_stub(
            &artifact.precompile_manifest,
            &artifact.required_property_requirements,
            &artifact.profile_catalog,
        )
        .expect("semantic hash"),
    );
}

#[derive(Clone, Copy)]
pub(crate) struct EmptySchemeFactory;

impl ColumnBackendFactory for EmptySchemeFactory {
    fn scheme_id(&self) -> SchemeId {
        SchemeId(0x1000)
    }

    fn name(&self) -> &str {
        "empty"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if setup.profile.scheme_profile.scheme_family_id != self.scheme_id() {
            return Err(ExtError::validation(format!(
                "empty scheme expected id {} but received {}",
                self.scheme_id().0,
                setup.profile.scheme_profile.scheme_family_id.0,
            )));
        }
        SsmcScheme::<3>.materialize_backend(setup)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct UnsupportedLayoutSchemeFactory;

impl ColumnBackendFactory for UnsupportedLayoutSchemeFactory {
    fn scheme_id(&self) -> SchemeId {
        SchemeId(0x1000)
    }

    fn name(&self) -> &str {
        "unsupported_layout"
    }

    fn materialize_backend(
        &self,
        _setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        Err(ExtError::validation("unsupported proof scheme layout"))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct UnsupportedPropertySchemeFactory;

impl ColumnBackendFactory for UnsupportedPropertySchemeFactory {
    fn scheme_id(&self) -> SchemeId {
        SchemeId(0x1001)
    }

    fn name(&self) -> &str {
        "unsupported_property"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        if !setup.required_property_query_kinds.is_empty() {
            return Err(ExtError::validation("unsupported property query"));
        }
        SmtScheme::<3>.materialize_backend(setup)
    }
}

fn compute_semantic_hash_stub(
    precompile_manifest: &[tabula_artifact::PrecompileDescriptor],
    required_property_requirements: &[tabula_ir::PropertyRequirement],
    profile_catalog: &ProfileCatalog,
) -> Result<[u8; 32], serde_json::Error> {
    #[derive(Serialize)]
    struct SemanticContract<'a> {
        precompile_manifest: &'a [tabula_artifact::PrecompileDescriptor],
        required_property_requirements: &'a [tabula_ir::PropertyRequirement],
        profile_catalog: &'a ProfileCatalog,
    }

    let payload = serde_json::to_vec(&SemanticContract {
        precompile_manifest,
        required_property_requirements,
        profile_catalog,
    })?;

    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_HASH_DOMAIN);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    Ok(hasher.finalize().into())
}

fn compute_profile_hash(
    schemas: &[tabula_core::TableSchema],
    tx_types: &[tabula_ir::TxTypeDef],
    profile_catalog: &ProfileCatalog,
) -> Result<[u8; 32], String> {
    let mut canonical_schemas = schemas.to_vec();
    canonical_schemas.sort_by_key(|schema| schema.id);
    for schema in &mut canonical_schemas {
        schema.columns.sort_by_key(|column| column.id);
    }

    let mut canonical_tx_types = tx_types.to_vec();
    canonical_tx_types.sort_by_key(|tx| tx.id);

    let mut hasher = blake3::Hasher::new();
    hasher.update(PROFILE_HASH_DOMAIN);
    hasher.update(&(canonical_schemas.len() as u32).to_be_bytes());
    for schema in &canonical_schemas {
        hasher.update(
            &borsh_to_vec(schema).map_err(|err| format!("failed to encode table schema: {err}"))?,
        );
    }
    hasher.update(&(canonical_tx_types.len() as u32).to_be_bytes());
    for tx in &canonical_tx_types {
        hasher.update(&borsh_to_vec(tx).map_err(|err| format!("failed to encode tx type: {err}"))?);
    }
    let profile_catalog_bytes = serde_json::to_vec(profile_catalog)
        .map_err(|err| format!("failed to encode profile catalog: {err}"))?;
    hasher.update(&(profile_catalog_bytes.len() as u32).to_be_bytes());
    hasher.update(&profile_catalog_bytes);
    Ok(*hasher.finalize().as_bytes())
}

fn replace_or_register_scheme_profile(
    profile_catalog: &mut ProfileCatalog,
    scheme_profile: SchemeProfile,
) -> SchemeProfileId {
    if scheme_profile.scheme_profile_id == SCHEME_PROFILE_SSMC_ID
        || scheme_profile.scheme_family_id == SchemeId::SSMC
    {
        return SCHEME_PROFILE_SSMC_ID;
    }
    if scheme_profile.scheme_profile_id == SCHEME_PROFILE_SMT_ID
        || scheme_profile.scheme_family_id == SchemeId::SMT
    {
        return SCHEME_PROFILE_SMT_ID;
    }

    let scheme_profile_id = scheme_profile.scheme_profile_id;
    profile_catalog
        .schemes
        .retain(|profile| profile.scheme_profile_id != scheme_profile_id);
    profile_catalog
        .register_scheme(scheme_profile)
        .expect("register custom scheme profile");
    scheme_profile_id
}

fn scheme_profile(
    scheme_profile_id: SchemeProfileId,
    scheme_id: SchemeId,
    layout_kind: ColumnLayoutKind,
    root_profile_id: RootProfileId,
    property_query_capabilities: Vec<PropertyQueryKind>,
) -> SchemeProfile {
    let (commitment_contract_kind, encoding_requirements) = match layout_kind {
        ColumnLayoutKind::SSMC_V1 => (
            CommitmentContractKind::SortedStateMerkleChain,
            builtin_ssmc_scheme_profile()
                .expect("built-in ssmc profile")
                .encoding_requirements,
        ),
        ColumnLayoutKind::SMT_V1 => (
            CommitmentContractKind::SparseMerkleTree,
            builtin_smt_scheme_profile()
                .expect("built-in smt profile")
                .encoding_requirements,
        ),
        other => (
            CommitmentContractKind::Opaque {
                family: format!("layout_{:04x}", other.0),
            },
            EncodingRequirements {
                field_family: FieldFamily::KoalaBear31,
                encoding_class: EncodingClass::FieldElementArray,
                width_constraint: WidthConstraint::Any,
                canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
                ordering_preserving: None,
            },
        ),
    };
    SchemeProfile::new(
        scheme_profile_id,
        format!("custom_scheme_{}", scheme_id.raw()),
        None,
        scheme_id,
        commitment_contract_kind,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        property_query_capabilities,
        encoding_requirements,
        layout_kind,
        root_profile_id,
    )
    .expect("custom scheme profile")
}
