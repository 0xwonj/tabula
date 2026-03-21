#![allow(missing_docs)]
#![cfg(all(feature = "verify", not(feature = "prove")))]

use tabula_sdk::Sdk;
use tabula_sdk::ext::PrecompileBundle;
use tabula_testing::extensions::precompile::ConstantOnePrecompileProofFactory;
use tabula_testing::fixtures::artifacts::{
    precompile_requirement_artifact, precompile_requirement_descriptor,
};

#[test]
fn verification_only_precompile_bundle_builds_verifier_without_handler() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile(
            PrecompileBundle::verification(
                descriptor.clone(),
                ConstantOnePrecompileProofFactory::new(descriptor),
            )
            .expect("verification bundle"),
        )
        .expect("register verification-only precompile")
        .build();

    let artifact = precompile_requirement_artifact();
    sdk.verifier(artifact.clone())
        .expect("sdk verifier")
        .warm()
        .expect("warm verifier");

    sdk.open(artifact)
        .expect("open artifact")
        .verifier()
        .expect("program verifier")
        .warm()
        .expect("warm program verifier");
}
