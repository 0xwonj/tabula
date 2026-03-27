#![allow(missing_docs)]
use std::collections::BTreeMap;

use borsh::BorshDeserialize;

use tabula_contract::{
    BINDING_REGISTRY_VERSION, BindingRegistry, BindingStatus, CONTRACT_RULES_V1,
    CONTRACT_SCHEMA_VERSION, ContractCompatibilityPolicy, ContractMetadataEnvelope,
    ContractRuleCode, ContractValidationError, PROOF_ENVELOPE_VERSION, PUBLIC_INPUT_FIELDS,
    ProofEncodingId, ProofEnvelopeV2, ProofStatement, ProofSystemId, PublicContextBinding,
    PublicInputField, PublicStatement, STATEMENT_SCHEMA_VERSION, StaticTableArtifact,
    StaticTableArtifactRow, VERIFIER_PROFILE_VERSION, access_bus_field_names, binding_registry,
    decode_proof_envelope, encode_proof_envelope,
};
use tabula_ir as ir;

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[test]
fn metadata_envelope_canonical_snapshot() {
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub: Some([0x22; 32]),
    };

    let canonical = envelope.to_canonical_bytes();
    let canonical_hash = envelope.canonical_hash();

    assert_eq!(
        to_hex(&canonical),
        "54434d4502111111111111111111111111111111111111111111111111111111111111111100000001000000020000000300000001012222222222222222222222222222222222222222222222222222222222222222"
    );
    assert_eq!(
        to_hex(&canonical_hash),
        "5931eacaeac6f22ecc4363d02053f99c907ba1675514557aa466af9a4f77f8a7"
    );
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_schema_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_binding_registry_version: BINDING_REGISTRY_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION + 1,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("newer/unknown schema version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownContractSchemaVersion {
            got: CONTRACT_SCHEMA_VERSION + 1,
        }
    );
    assert_eq!(err.code(), "unknown_contract_schema_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_binding_registry_version() {
    let unknown_binding_registry_version = BINDING_REGISTRY_VERSION + 1;
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_binding_registry_version: BINDING_REGISTRY_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: unknown_binding_registry_version,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown binding version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownBindingRegistryVersion {
            got: unknown_binding_registry_version,
        }
    );
    assert_eq!(err.code(), "unknown_binding_registry_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_profile_mismatch() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_binding_registry_version: BINDING_REGISTRY_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash_stub: Some([0x33; 32]),
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x22; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub: Some([0x33; 32]),
    };

    let err = policy
        .validate(&envelope)
        .expect_err("profile mismatch must hard-fail");
    assert_eq!(err, ContractValidationError::ProfileMismatch);
    assert_eq!(err.code(), "profile_mismatch");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_statement_schema_version() {
    let unknown_statement_schema_version = STATEMENT_SCHEMA_VERSION + 1;
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_binding_registry_version: BINDING_REGISTRY_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: unknown_statement_schema_version,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown statement schema version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownStatementSchemaVersion {
            got: unknown_statement_schema_version,
        }
    );
    assert_eq!(err.code(), "unknown_statement_schema_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_verifier_profile_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_binding_registry_version: BINDING_REGISTRY_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION + 1,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown verifier profile version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownVerifierProfileVersion {
            got: VERIFIER_PROFILE_VERSION + 1,
        }
    );
    assert_eq!(err.code(), "unknown_verifier_profile_version");
}

#[test]
fn binding_registry_is_complete() {
    let registry = binding_registry();
    registry
        .validate_completeness()
        .expect("default binding registry must be complete");

    for field in PUBLIC_INPUT_FIELDS {
        assert!(
            registry.bindings.contains_key(&field),
            "missing field in default binding registry: {field:?}"
        );
    }
}

#[test]
fn binding_registry_detects_missing_field() {
    let mut bindings = BTreeMap::new();
    for field in PUBLIC_INPUT_FIELDS {
        if field != PublicInputField::Budgets {
            bindings.insert(field, BindingStatus::BoundInAir);
        }
    }
    let registry = BindingRegistry {
        version: BINDING_REGISTRY_VERSION,
        bindings,
    };

    let err = registry
        .validate_completeness()
        .expect_err("missing field must fail completeness check");
    match err {
        ContractValidationError::IncompleteBinding { missing_fields } => {
            assert_eq!(missing_fields, vec![PublicInputField::Budgets]);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn static_table_artifact_round_trips_and_stays_root_exported() {
    let artifact = StaticTableArtifact {
        rows: vec![StaticTableArtifactRow {
            relation_id: 7,
            input_digest: [1; 8],
            output_digest: [2; 8],
        }],
        root: [3; 32],
    };

    let encoded = borsh::to_vec(&artifact).expect("encode static table artifact");
    let decoded =
        StaticTableArtifact::try_from_slice(&encoded).expect("decode static table artifact");

    assert_eq!(decoded, artifact);
}

#[test]
fn access_bus_field_names_include_tx_index() {
    let expected = vec![
        "table_id".to_string(),
        "col_id".to_string(),
        "key_limb0".to_string(),
        "key_limb1".to_string(),
        "key_limb2".to_string(),
        "tx_index".to_string(),
        "value[0]".to_string(),
        "value[1]".to_string(),
        "value[2]".to_string(),
        "is_null".to_string(),
    ];
    assert_eq!(access_bus_field_names(3), expected);
}

fn sample_statement() -> ProofStatement {
    ProofStatement::new(
        tabula_contract::ProgramBinding::new([0xaa; 32], [0xbb; 32]),
        PublicStatement {
            program_id: ir::ProgramId(7),
            public_context: vec![PublicContextBinding {
                field: ir::ContextFieldId(3),
                value: tabula_core::PortableValue::new(tabula_core::TypeId(1), vec![0x2a]),
            }],
            event_digest: [0x11; 32],
        },
        [0x22; 32],
        [0x33; 32],
        [0x44; 32],
        [0x55; 32],
    )
}

#[test]
fn proof_statement_canonical_hash_is_stable() {
    let statement = sample_statement();
    let canonical = statement.canonical_bytes().expect("canonical bytes");
    let hash = statement.statement_hash_bytes().expect("statement hash");

    assert_eq!(
        to_hex(&canonical),
        "746162756c612e636f6e74726163742e70726f6f665f73746174656d656e7403000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb07000000010000000300000001000000010000002a11111111111111111111111111111111111111111111111111111111111111112222222222222222222222222222222222222222222222222222222222222222333333333333333333333333333333333333333333333333333333333333333344444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555"
    );
    assert_eq!(
        to_hex(&hash),
        "eb08c6e0edef60f84c08ca9c6a727fc36079ebf3c575a2fe58d64c849f5866a8"
    );
    assert_eq!(
        statement.schema_version, STATEMENT_SCHEMA_VERSION,
        "statement should use the current contract schema version"
    );
}

#[test]
fn program_binding_json_round_trips_with_hex_strings() {
    let binding = tabula_contract::ProgramBinding::new([0xaa; 32], [0xbb; 32]);
    let json = serde_json::to_string(&binding).expect("serialize binding");

    assert_eq!(
        json,
        "{\"program_hash\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"metadata_hash\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}"
    );

    let decoded: tabula_contract::ProgramBinding =
        serde_json::from_str(&json).expect("deserialize binding");
    assert_eq!(decoded, binding);
}

#[test]
fn program_binding_json_rejects_uppercase_hex() {
    let err = serde_json::from_str::<tabula_contract::ProgramBinding>(
        "{\"program_hash\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\"metadata_hash\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}",
    )
    .expect_err("uppercase hex must be rejected");
    assert!(err.to_string().contains("lowercase hex"));
}

#[test]
fn proof_envelope_round_trips() {
    let envelope = ProofEnvelopeV2::new(
        sample_statement(),
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V2,
        vec![0xde, 0xad, 0xbe, 0xef],
    );

    let encoded = encode_proof_envelope(&envelope).expect("encode proof envelope");
    let decoded = decode_proof_envelope(&encoded).expect("decode proof envelope");

    assert_eq!(decoded, envelope);
}

#[test]
fn proof_envelope_rejects_unknown_version() {
    let envelope = ProofEnvelopeV2::new(
        sample_statement(),
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V2,
        vec![1, 2, 3],
    );
    let mut encoded = encode_proof_envelope(&envelope).expect("encode proof envelope");
    let start = b"tabula.contract.proof".len();
    encoded[start..start + 4].copy_from_slice(&(PROOF_ENVELOPE_VERSION + 1).to_be_bytes());

    let err = decode_proof_envelope(&encoded).expect_err("unknown proof envelope version");
    assert!(matches!(
        err,
        tabula_contract::ProofContractError::ContractValidation(
            ContractValidationError::UnknownProofEnvelopeVersion { .. }
        )
    ));
}

#[test]
fn proof_envelope_rejects_unknown_proof_system() {
    let mut envelope = ProofEnvelopeV2::new(
        sample_statement(),
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V2,
        vec![1, 2, 3],
    );
    envelope.proof_system = borsh::from_slice(&borsh::to_vec(&2u16).expect("encode u16"))
        .expect("decode invalid proof system id");
    let err = encode_proof_envelope(&envelope).expect_err("unknown proof system id");
    assert!(matches!(
        err,
        tabula_contract::ProofContractError::UnknownProofSystemId { got: 2 }
    ));
}

#[test]
fn proof_envelope_rejects_unknown_proof_encoding() {
    let mut envelope = ProofEnvelopeV2::new(
        sample_statement(),
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V2,
        vec![1, 2, 3],
    );
    envelope.proof_encoding = borsh::from_slice(&borsh::to_vec(&9u16).expect("encode u16"))
        .expect("decode invalid proof encoding id");
    let err = encode_proof_envelope(&envelope).expect_err("unknown proof encoding id");
    assert!(matches!(
        err,
        tabula_contract::ProofContractError::UnknownProofEncodingId { got: 9 }
    ));
}

#[test]
fn delete_all_rule_is_registered() {
    assert!(
        CONTRACT_RULES_V1
            .iter()
            .any(|rule| rule.code == ContractRuleCode::ComNewRequiresNonEmptyNewSet)
    );
}
