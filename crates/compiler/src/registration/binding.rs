use blake3::Hasher;
use sha2::Digest as _;

use tabula_contract::{ContractMetadataEnvelope, ProgramBinding};
use tabula_core::ProgramExecutionContract;
use tabula_ir as ir;
use tabula_profile::ProfileCatalog;

pub(crate) fn compute_profile_hash(
    execution_contract: &ProgramExecutionContract,
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    let mut hasher = Hasher::new();
    hasher.update(b"tabula.driver.profile_hash.v1");
    hasher.update(&borsh::to_vec(execution_contract)?);
    let profile_catalog_bytes = serde_json::to_vec(profile_catalog)?;
    hasher.update(&(profile_catalog_bytes.len() as u32).to_be_bytes());
    hasher.update(&profile_catalog_bytes);
    Ok(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_semantic_hash(
    program: &ir::Program,
    execution_contract: &ProgramExecutionContract,
    profile_catalog: &ProfileCatalog,
) -> anyhow::Result<[u8; 32]> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"tabula.driver.semantic_hash.v1");
    hasher.update(&borsh::to_vec(execution_contract)?);
    hasher.update(&borsh::to_vec(program)?);
    hasher.update(serde_json::to_vec(profile_catalog)?);
    Ok(hasher.finalize().into())
}

pub(crate) fn compute_program_binding(
    program: &ir::Program,
    execution_contract: &ProgramExecutionContract,
    metadata_envelope: &ContractMetadataEnvelope,
) -> anyhow::Result<ProgramBinding> {
    let mut hasher = Hasher::new();
    hasher.update(b"tabula.contract.program_binding.v1");
    hasher.update(&borsh::to_vec(program)?);
    hasher.update(&borsh::to_vec(execution_contract)?);
    Ok(ProgramBinding::new(
        *hasher.finalize().as_bytes(),
        metadata_envelope.canonical_hash_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use tabula_contract::{
        CONTRACT_SCHEMA_VERSION, STATEMENT_SCHEMA_VERSION, VERIFIER_PROFILE_VERSION,
    };
    use tabula_core::{
        ColId, ColumnProfileId, CommittedKeyLayout, EncodingProfileId, KeyComponentSchema,
        KeyOrderingFamily, ProgramExecutionContract, ProgramMachineShape, PropertyQueryKind,
        StateColumnContract, StateContract, StateTableContract, TableId, TableKeyContract, TypeId,
    };
    use tabula_profile::builtin_catalog;

    fn minimal_program() -> ir::Program {
        ir::Program {
            program_id: ir::ProgramId(7),
            state: ir::StateSchema { tables: vec![] },
            context: ir::ContextSchema { fields: vec![] },
            const_pool: ir::ConstantPool { entries: vec![] },
            relation_manifest: ir::RelationManifest { entries: vec![] },
            capability_manifest: ir::CapabilityManifest { entries: vec![] },
            event_manifest: ir::EventManifest { entries: vec![] },
            entries: vec![],
        }
    }

    fn metadata_envelope() -> ContractMetadataEnvelope {
        ContractMetadataEnvelope {
            profile_hash: [0x11; 32],
            contract_schema_version: CONTRACT_SCHEMA_VERSION,
            statement_schema_version: STATEMENT_SCHEMA_VERSION,
            verifier_profile_version: VERIFIER_PROFILE_VERSION,
            semantic_hash: [0x22; 32],
        }
    }

    fn table_key_contract(
        component_types: Vec<TypeId>,
        component_encoding_profile_ids: Vec<EncodingProfileId>,
    ) -> TableKeyContract {
        TableKeyContract {
            components: component_types
                .into_iter()
                .enumerate()
                .map(|(index, ty)| KeyComponentSchema {
                    symbol: format!("k{index}"),
                    ty,
                })
                .collect(),
            component_encoding_profile_ids,
            ordering_family: KeyOrderingFamily::LexicographicByComponent,
            committed_layout: CommittedKeyLayout {
                byte_width: 8,
                fe_width: 3,
            },
        }
    }

    fn execution_contract(
        component_types: Vec<TypeId>,
        component_encoding_profile_ids: Vec<EncodingProfileId>,
        shape: ProgramMachineShape,
    ) -> ProgramExecutionContract {
        ProgramExecutionContract {
            state: StateContract {
                tables: vec![StateTableContract {
                    id: TableId(1),
                    name: "users".into(),
                    key: table_key_contract(component_types, component_encoding_profile_ids),
                    columns: vec![StateColumnContract {
                        id: ColId(0),
                        name: "balance".into(),
                        ty: TypeId(0),
                        column_profile_id: ColumnProfileId(0),
                        required_property_queries: BTreeSet::from([PropertyQueryKind::Successor]),
                    }],
                }],
            },
            machine_shape: shape,
        }
    }

    #[test]
    fn profile_hash_changes_when_table_key_schema_changes() {
        let catalog = builtin_catalog().unwrap();
        let left = compute_profile_hash(
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(0)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &catalog,
        )
        .unwrap();
        let right = compute_profile_hash(
            &execution_contract(
                vec![TypeId(2)],
                vec![EncodingProfileId(2)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &catalog,
        )
        .unwrap();

        assert_ne!(left, right);
    }

    #[test]
    fn profile_hash_changes_when_key_encoding_changes() {
        let catalog = builtin_catalog().unwrap();
        let left = compute_profile_hash(
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(0)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &catalog,
        )
        .unwrap();
        let right = compute_profile_hash(
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(99)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &catalog,
        )
        .unwrap();

        assert_ne!(left, right);
    }

    #[test]
    fn program_binding_changes_when_key_contract_changes() {
        let program = minimal_program();
        let metadata = metadata_envelope();
        let left = compute_program_binding(
            &program,
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(0)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &metadata,
        )
        .unwrap();
        let right = compute_program_binding(
            &program,
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(99)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &metadata,
        )
        .unwrap();

        assert_ne!(left, right);
    }

    #[test]
    fn program_binding_changes_when_machine_shape_changes() {
        let program = minimal_program();
        let metadata = metadata_envelope();
        let left = compute_program_binding(
            &program,
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(0)],
                ProgramMachineShape {
                    max_slots: 0,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &metadata,
        )
        .unwrap();
        let right = compute_program_binding(
            &program,
            &execution_contract(
                vec![TypeId(0)],
                vec![EncodingProfileId(0)],
                ProgramMachineShape {
                    max_slots: 4,
                    max_key_components: 1,
                    max_key_fes: 3,
                },
            ),
            &metadata,
        )
        .unwrap();

        assert_ne!(left, right);
    }
}
