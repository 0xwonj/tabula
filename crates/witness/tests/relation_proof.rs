#![allow(missing_docs)]
use tabula_contract::{StaticTableArtifact, StaticTableArtifactRow};
use tabula_core::TypeId;
use tabula_ir as ir;
use tabula_witness::{
    PreparedRelationTableRow, RelationClaim, RelationClaimKind, prepare_relation_proof,
};

fn relation_program(inputs: Vec<TypeId>, outputs: Vec<TypeId>) -> ir::Program {
    ir::Program {
        program_id: ir::ProgramId(0),
        state: ir::StateSchema { tables: vec![] },
        context: ir::ContextSchema { fields: vec![] },
        const_pool: ir::ConstantPool { entries: vec![] },
        relation_manifest: ir::RelationManifest {
            entries: vec![ir::RelationManifestEntry {
                id: ir::RelationId(0),
                descriptor: ir::RelationDescriptor {
                    symbol: "r".to_owned(),
                    inputs,
                    outputs,
                },
                binding: ir::RelationBinding::Map { rows: vec![] },
            }],
        },
        capability_manifest: ir::CapabilityManifest { entries: vec![] },
        event_manifest: ir::EventManifest { entries: vec![] },
        entries: vec![],
    }
}

#[test]
fn prepares_lookup_multiplicities_from_relation_claims() {
    let program = relation_program(vec![tabula_profile::TYPE_U64_ID], vec![]);
    let artifact = StaticTableArtifact {
        rows: vec![StaticTableArtifactRow {
            relation_id: 0,
            input_digest: [11; 8],
            output_digest: [0; 8],
        }],
        root: [7; 32],
    };
    let claims = vec![RelationClaim {
        relation: tabula_ir::RelationId(0),
        kind: RelationClaimKind::Assert,
        inputs: vec![tabula_types::u64_typed(9)],
        input_digest: [11; 8],
        outputs: vec![],
        output_digest: [0; 8],
        tx_index: 0,
        effect_ordinal_in_tx: 1,
        op_index: 3,
    }];

    let prepared = prepare_relation_proof(&program, &artifact, &claims).expect("prepare proof");

    assert_eq!(prepared.root(), artifact.root);
    assert_eq!(
        prepared.table_rows(),
        &[PreparedRelationTableRow {
            relation_id: 0,
            input_digest: [11; 8],
            output_digest: [0; 8],
            lookup_mult: 1,
        }]
    );
}

#[test]
fn rejects_claims_missing_from_the_sealed_manifest() {
    let program = relation_program(vec![tabula_profile::TYPE_U64_ID], vec![]);
    let artifact = StaticTableArtifact {
        rows: vec![],
        root: [0; 32],
    };
    let claims = vec![RelationClaim {
        relation: tabula_ir::RelationId(0),
        kind: RelationClaimKind::Assert,
        inputs: vec![tabula_types::u64_typed(1)],
        input_digest: [1; 8],
        outputs: vec![],
        output_digest: [0; 8],
        tx_index: 0,
        effect_ordinal_in_tx: 0,
        op_index: 0,
    }];

    let err = prepare_relation_proof(&program, &artifact, &claims).expect_err("missing row");

    assert!(
        err.to_string().contains("sealed manifest"),
        "unexpected error: {err}"
    );
}
