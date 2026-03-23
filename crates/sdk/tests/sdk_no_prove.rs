#![allow(missing_docs)]
#![cfg(not(feature = "prove"))]

use tabula_compiler::ProgramDefinition;
use tabula_core::TxTypeId;
use tabula_ir::{Instruction, TxTypeDef};
use tabula_sdk::ext::PrecompileId;
use tabula_sdk::{Sdk, SdkError};
use tabula_testing::extensions::precompile::constant_one_precompile_descriptor;
use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_testing::fixtures::state::empty_state;

#[test]
fn capability_backed_execution_requires_prove_feature() {
    let sdk = Sdk::standard();
    let program = sdk
        .open(precompile_requirement_artifact())
        .expect("open artifact");

    let err = program
        .execute(&empty_state(), &single_tx_batch(1, vec![]))
        .expect_err("capability-backed execution should fail without prove");

    match err {
        SdkError::FeatureDisabled { feature, detail } => {
            assert_eq!(feature, "prove");
            assert!(detail.contains("capability-backed execution"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn descriptor_registration_without_verify_supports_source_sealing() {
    let descriptor = constant_one_precompile_descriptor(PrecompileId(0x0001));
    let definition = ProgramDefinition {
        table_schemas: vec![],
        tx_types: vec![TxTypeDef {
            id: TxTypeId(1),
            name: "scan".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Precompile {
                id: PrecompileId(0x0001),
                dst_slots: vec![0],
                inputs: vec![],
            }],
        }],
        column_schemes: vec![],
    };

    let sdk = Sdk::builder()
        .with_precompile_descriptor(descriptor)
        .expect("register descriptor without verify")
        .build();
    let program = sdk
        .register(&definition)
        .expect("register precompile-backed source");

    assert_eq!(program.artifact().precompile_manifest.len(), 1);
    assert_eq!(
        program.artifact().precompile_manifest[0].precompile_id,
        PrecompileId(0x0001)
    );
}
