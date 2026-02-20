use std::collections::BTreeMap;

use tabula_proof::contract::{
    APPLY_BATCH_FIELDS, ApplyBatchField, C10_READ_ACCESS_SCHEMA_VERSION_V2,
    C11_WRITE_ACCESS_SCHEMA_VERSION_V2, CONTRACT_RULES_V1, CONTRACT_SCHEMA_VERSION_V1,
    ContractCompatibilityPolicy, ContractMetadataEnvelope, ContractRuleCode,
    ContractValidationError, STATEMENT_BINDING_VERSION_V1, StatementBindingRegistry,
    StatementBindingStatus, access_bus_field_names, apply_batch_binding_registry_v1,
};

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

#[test]
fn metadata_envelope_canonical_snapshot() {
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        statement_binding_version: STATEMENT_BINDING_VERSION_V1,
        semantic_hash_stub: Some([0x22; 32]),
    };

    let canonical = envelope.to_canonical_bytes();
    let canonical_hash = envelope.canonical_hash();

    assert_eq!(
        to_hex(&canonical),
        "54434d450111111111111111111111111111111111111111111111111111111111111111110000000100000001012222222222222222222222222222222222222222222222222222222222222222"
    );
    assert_eq!(
        to_hex(&canonical_hash),
        "499d02e91beb8160b42407dbcb8415d8175c4542dafab903812eac614e9cbb11"
    );
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_schema_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_statement_binding_version: STATEMENT_BINDING_VERSION_V1,
        expected_semantic_hash_stub: None,
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1 + 1,
        statement_binding_version: STATEMENT_BINDING_VERSION_V1,
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
fn metadata_validation_is_fail_closed_for_profile_mismatch() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        expected_statement_binding_version: STATEMENT_BINDING_VERSION_V1,
        expected_semantic_hash_stub: Some([0x33; 32]),
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x22; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        statement_binding_version: STATEMENT_BINDING_VERSION_V1,
        semantic_hash_stub: Some([0x33; 32]),
    };

    let err = policy
        .validate(&envelope)
        .expect_err("profile mismatch must hard-fail");
    assert_eq!(err, ContractValidationError::ProfileMismatch);
    assert_eq!(err.code(), "profile_mismatch");
}

#[test]
fn statement_binding_registry_is_complete() {
    let registry = apply_batch_binding_registry_v1();
    registry
        .validate_completeness()
        .expect("default v1 binding registry must be complete");

    for field in APPLY_BATCH_FIELDS {
        assert!(
            registry.bindings.contains_key(&field),
            "missing field in default binding registry: {:?}",
            field
        );
    }
}

#[test]
fn statement_binding_registry_detects_missing_field() {
    let mut bindings = BTreeMap::new();
    for field in APPLY_BATCH_FIELDS {
        if field != ApplyBatchField::Budgets {
            bindings.insert(field, StatementBindingStatus::BoundInAir);
        }
    }
    let registry = StatementBindingRegistry {
        version: STATEMENT_BINDING_VERSION_V1,
        bindings,
    };

    let err = registry
        .validate_completeness()
        .expect_err("missing field must fail completeness check");
    match err {
        ContractValidationError::IncompleteStatementBinding { missing_fields } => {
            assert_eq!(missing_fields, vec![ApplyBatchField::Budgets]);
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
