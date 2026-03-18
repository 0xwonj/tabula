#![allow(missing_docs)]
use std::collections::BTreeMap;

use tabula_contract::{
    BINDING_VERSION_V1, BindingRegistry, BindingStatus, C10_READ_ACCESS_SCHEMA_VERSION_V2,
    C11_WRITE_ACCESS_SCHEMA_VERSION_V2, CONTRACT_RULES_V1, CONTRACT_SCHEMA_VERSION_V1,
    ContractCompatibilityPolicy, ContractMetadataEnvelope, ContractRuleCode,
    ContractValidationError, PUBLIC_INPUT_FIELDS, PublicInputField, STATEMENT_SCHEMA_VERSION_V1,
    VERIFIER_PROFILE_VERSION_V1, access_bus_field_names, binding_registry_v1,
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
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: Some([0x22; 32]),
    };

    let canonical = envelope.to_canonical_bytes();
    let canonical_hash = envelope.canonical_hash();

    assert_eq!(
        to_hex(&canonical),
        "54434d4502111111111111111111111111111111111111111111111111111111111111111100000001000000010000000100000001012222222222222222222222222222222222222222222222222222222222222222"
    );
    assert_eq!(
        to_hex(&canonical_hash),
        "da2d64ac8df9c3eb01aa45c0794c97b24123b7dec136a25ff444f569456d36a5"
    );
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_schema_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_binding_version: BINDING_VERSION_V1,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1 + 1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("newer/unknown schema version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownContractSchemaVersion {
            got: CONTRACT_SCHEMA_VERSION_V1 + 1,
        }
    );
    assert_eq!(err.code(), "unknown_contract_schema_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_binding_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_binding_version: BINDING_VERSION_V1,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1 + 1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown binding version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownBindingVersion {
            got: BINDING_VERSION_V1 + 1,
        }
    );
    assert_eq!(err.code(), "unknown_binding_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_profile_mismatch() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_binding_version: BINDING_VERSION_V1,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        expected_semantic_hash_stub: Some([0x33; 32]),
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x22; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
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
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_binding_version: BINDING_VERSION_V1,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1 + 1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown statement schema version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownStatementSchemaVersion {
            got: STATEMENT_SCHEMA_VERSION_V1 + 1,
        }
    );
    assert_eq!(err.code(), "unknown_statement_schema_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_verifier_profile_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_binding_version: BINDING_VERSION_V1,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1 + 1,
        semantic_hash_stub: None,
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown verifier profile version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownVerifierProfileVersion {
            got: VERIFIER_PROFILE_VERSION_V1 + 1,
        }
    );
    assert_eq!(err.code(), "unknown_verifier_profile_version");
}

#[test]
fn binding_registry_is_complete() {
    let registry = binding_registry_v1();
    registry
        .validate_completeness()
        .expect("default v1 binding registry must be complete");

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
        version: BINDING_VERSION_V1,
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
fn c10_c11_schema_snapshot_v2_with_tx_index() {
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
    assert_eq!(C10_READ_ACCESS_SCHEMA_VERSION_V2, 2);
    assert_eq!(C11_WRITE_ACCESS_SCHEMA_VERSION_V2, 2);
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
