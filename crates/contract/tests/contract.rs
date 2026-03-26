#![allow(missing_docs)]
use std::collections::BTreeMap;

use borsh::BorshDeserialize;

use tabula_contract::{
    BINDING_REGISTRY_VERSION, BindingRegistry, BindingStatus, CONTRACT_RULES_V1,
    CONTRACT_SCHEMA_VERSION, ContractCompatibilityPolicy, ContractMetadataEnvelope,
    ContractRuleCode, ContractValidationError, PUBLIC_INPUT_FIELDS, PublicInputField,
    STATEMENT_SCHEMA_VERSION, StaticTableArtifact, StaticTableArtifactRow,
    VERIFIER_PROFILE_VERSION, access_bus_field_names, binding_registry,
};

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
        "54434d4502111111111111111111111111111111111111111111111111111111111111111100000001000000020000000200000001012222222222222222222222222222222222222222222222222222222222222222"
    );
    assert_eq!(
        to_hex(&canonical_hash),
        "da24e8d6e5b1a25d990cbe27de11b700d5d96ab59f8b237961b638d26d732150"
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

#[test]
fn delete_all_rule_is_registered() {
    assert!(
        CONTRACT_RULES_V1
            .iter()
            .any(|rule| rule.code == ContractRuleCode::ComNewRequiresNonEmptyNewSet)
    );
}
