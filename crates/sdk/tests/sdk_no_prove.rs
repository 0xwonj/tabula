#![allow(missing_docs)]
#![cfg(not(feature = "prove"))]

use tabula_sdk::{Sdk, SdkError};
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
