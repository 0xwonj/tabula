#![allow(missing_docs)]

use borsh::BorshDeserialize;
use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;
use tabula_contract::{
    ArtifactContext, BoundStatement, CONTRACT_RULES, CONTRACT_SCHEMA_VERSION,
    ContractCompatibilityPolicy, ContractMetadataEnvelope, ContractRuleCode,
    ContractValidationError, PROOF_ENVELOPE_VERSION, ProgramBinding, ProofEncodingId,
    ProofEnvelope, ProofSystemId, PublicStatement, STATEMENT_SCHEMA_VERSION, StaticTableArtifact,
    StaticTableArtifactRow, VERIFIER_PROFILE_VERSION, access_bus_field_names,
    decode_proof_envelope, encode_proof_envelope,
};
use tabula_core::ProgramId;
use tabula_core::execution::NATIVE_MAX_KEY_FES;

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn digest(words: [u32; 8]) -> NativeDigest {
    NativeDigest(words.map(KoalaBear::new))
}

fn sample_public_statement() -> PublicStatement {
    PublicStatement {
        old_root: digest([1, 2, 3, 4, 5, 6, 7, 8]),
        new_root: digest([11, 12, 13, 14, 15, 16, 17, 18]),
        public_context_digest: digest([21, 22, 23, 24, 25, 26, 27, 28]),
        applied_tx_digest: digest([31, 32, 33, 34, 35, 36, 37, 38]),
        event_digest: digest([41, 42, 43, 44, 45, 46, 47, 48]),
    }
}

#[test]
fn contract_version_constants_match_native_clean_break() {
    assert_eq!(CONTRACT_SCHEMA_VERSION, 1);
    assert_eq!(STATEMENT_SCHEMA_VERSION, 1);
    assert_eq!(VERIFIER_PROFILE_VERSION, 1);
    assert_eq!(PROOF_ENVELOPE_VERSION, 1);
}

#[test]
fn metadata_envelope_canonical_snapshot_is_stable() {
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash: [0x22; 32],
    };

    let canonical = envelope.to_canonical_bytes();
    assert_eq!(&canonical[..5], b"TCME\x01");
    assert_eq!(
        canonical.len(),
        4 + 1 + 32 + 4 + 4 + 4 + 32,
        "metadata envelope layout must stay fixed"
    );
    assert_eq!(envelope.canonical_hash(), envelope.canonical_hash_bytes());
    assert_eq!(
        envelope.canonical_hash_hex(),
        to_hex(&envelope.canonical_hash())
    );
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_contract_schema_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash: [0x22; 32],
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION + 1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash: [0x22; 32],
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
fn metadata_validation_is_fail_closed_for_profile_mismatch() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash: [0x33; 32],
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x22; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash: [0x33; 32],
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
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash: [0x22; 32],
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION + 1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash: [0x22; 32],
    };

    let err = policy
        .validate(&envelope)
        .expect_err("unknown statement schema version must hard-fail");
    assert_eq!(
        err,
        ContractValidationError::UnknownStatementSchemaVersion {
            got: STATEMENT_SCHEMA_VERSION + 1,
        }
    );
    assert_eq!(err.code(), "unknown_statement_schema_version");
}

#[test]
fn metadata_validation_is_fail_closed_for_unknown_verifier_profile_version() {
    let policy = ContractCompatibilityPolicy {
        expected_profile_hash: [0x11; 32],
        expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
        expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
        expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
        expected_semantic_hash: [0x22; 32],
    };
    let envelope = ContractMetadataEnvelope {
        profile_hash: [0x11; 32],
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION + 1,
        semantic_hash: [0x22; 32],
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
    let mut expected = vec!["table_id".to_string(), "col_id".to_string()];
    expected
        .extend((0..usize::from(NATIVE_MAX_KEY_FES)).map(|index| format!("key_payload[{index}]")));
    expected.extend([
        "tx_index".to_string(),
        "value[0]".to_string(),
        "value[1]".to_string(),
        "value[2]".to_string(),
        "is_null".to_string(),
    ]);
    assert_eq!(access_bus_field_names(3), expected);
}

#[test]
fn bound_statement_canonical_hash_is_deterministic() {
    let statement = BoundStatement::new(
        ArtifactContext::new(
            ProgramBinding::new([0xaa; 32], [0xbb; 32]),
            ProgramId(7),
            [0x33; 32],
        ),
        sample_public_statement(),
    );

    let canonical = statement.canonical_bytes().expect("canonical bytes");
    let hash = statement.binding_digest().expect("binding digest");

    assert!(canonical.starts_with(b"tabula.contract.artifact_bound_statement"));
    assert_eq!(statement.schema_version, STATEMENT_SCHEMA_VERSION);
    assert_eq!(hash.len(), 32);
    assert_eq!(
        hash,
        statement
            .binding_digest()
            .expect("binding digest is deterministic")
    );
}

#[test]
fn program_binding_json_round_trips_with_hex_strings() {
    let binding = ProgramBinding::new([0xaa; 32], [0xbb; 32]);
    let json = serde_json::to_string(&binding).expect("serialize binding");

    assert_eq!(
        json,
        "{\"program_hash\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"metadata_hash\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}"
    );

    let decoded: ProgramBinding = serde_json::from_str(&json).expect("deserialize binding");
    assert_eq!(decoded, binding);
}

#[test]
fn program_binding_json_rejects_uppercase_hex() {
    let err = serde_json::from_str::<ProgramBinding>(
        "{\"program_hash\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\"metadata_hash\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}",
    )
    .expect_err("uppercase hex must be rejected");
    assert!(err.to_string().contains("lowercase hex"));
}

#[test]
fn proof_envelope_round_trips() {
    let envelope = ProofEnvelope::new(
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V1,
        vec![0xde, 0xad, 0xbe, 0xef],
    );

    let encoded = encode_proof_envelope(&envelope).expect("encode proof envelope");
    let decoded = decode_proof_envelope(&encoded).expect("decode proof envelope");

    assert_eq!(decoded, envelope);
}

#[test]
fn bound_statement_binding_digest_changes_with_context() {
    let baseline = BoundStatement::new(
        ArtifactContext::new(
            ProgramBinding::new([0xaa; 32], [0xbb; 32]),
            ProgramId(7),
            [0x33; 32],
        ),
        sample_public_statement(),
    )
    .binding_digest()
    .expect("baseline digest");
    let changed = BoundStatement::new(
        ArtifactContext::new(
            ProgramBinding::new([0xab; 32], [0xbb; 32]),
            ProgramId(7),
            [0x33; 32],
        ),
        sample_public_statement(),
    )
    .binding_digest()
    .expect("changed digest");

    assert_ne!(baseline, changed);
}

#[test]
fn proof_envelope_rejects_unknown_version() {
    let envelope = ProofEnvelope::new(
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V1,
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
    let mut envelope = ProofEnvelope::new(
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V1,
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
    let mut envelope = ProofEnvelope::new(
        ProofSystemId::TABULA_STARK,
        ProofEncodingId::TABULA_MACHINE_BINARY_V1,
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
        CONTRACT_RULES
            .iter()
            .any(|rule| rule.code == ContractRuleCode::ComNewRequiresNonEmptyNewSet)
    );
}
