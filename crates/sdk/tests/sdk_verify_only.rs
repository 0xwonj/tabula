#![allow(missing_docs)]
#![cfg(all(feature = "verify", not(feature = "prove")))]

use tabula_sdk::Sdk;
use tabula_sdk::ext::PrecompileBackendFactoryBundle;
use tabula_testing::extensions::precompile::ConstantOnePrecompileBackendFactory;
use tabula_testing::fixtures::artifacts::{
    precompile_requirement_artifact, precompile_requirement_descriptor,
};

#[test]
fn verification_only_precompile_backend_builds_verifier() {
    let descriptor = precompile_requirement_descriptor();
    let sdk = Sdk::builder()
        .with_precompile_support(
            descriptor.clone(),
            PrecompileBackendFactoryBundle::new(ConstantOnePrecompileBackendFactory::new(
                descriptor,
            )),
        )
        .expect("register verification precompile support")
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
