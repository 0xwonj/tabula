#![allow(missing_docs)]
#![cfg(not(feature = "prove"))]

#[cfg(not(feature = "compile"))]
use tabula_compiler::{CompilerCatalogs, compile_program_source_with_catalogs};
use tabula_ir as ir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_sdk::Sdk;
use tabula_sdk::interop::ArtifactExt;
#[cfg(not(feature = "compile"))]
use tabula_sdk::interop::register_compiled;

const NO_PROVE_SOURCE: &str = r#"
program NoProve

state {
  table accounts(key id: u64) {
    balance: u64 @ssmc;
  }
}

tx touch(id: u64) {
  let balance = accounts[id].balance;
  assert balance >= 0;
  return;
}
"#;

const CAPABILITY_SOURCE: &str = r#"
use capability demo_hash;

program CapabilityOnly

tx scan(id: u64) {
  let digest = demo_hash(id);
  assert true;
  return;
}
"#;

#[cfg(feature = "compile")]
fn compile_artifact(sdk: &Sdk, source: &str) -> tabula_sdk::Artifact {
    sdk.compile(source).expect("compile source")
}

#[cfg(not(feature = "compile"))]
fn compile_artifact(sdk: &Sdk, source: &str) -> tabula_sdk::Artifact {
    let compiled = compile_program_source_with_catalogs(source, &CompilerCatalogs::standard())
        .expect("compile source");
    register_compiled(sdk, compiled).expect("register compiled program")
}

#[test]
fn compile_and_open_artifact_without_prove() {
    let sdk = Sdk::standard();
    let artifact = compile_artifact(&sdk, NO_PROVE_SOURCE);
    let reopened = sdk.open(artifact.clone()).expect("open artifact");

    assert_eq!(artifact.digest(), reopened.artifact().digest());
}

#[test]
fn extension_capability_registration_supports_source_sealing() {
    let descriptor = tabula_compiler::SourceCapabilityDescriptor {
        path: "demo_hash".into(),
        inputs: vec![TYPE_U64_ID],
        outputs: vec![TYPE_BYTES32_ID],
        totality: ir::CapabilityTotality::Total,
        query_policy: ir::CapabilityQueryPolicy::QuerySafe,
        proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
        hash_family: None,
    };
    let extension = tabula_ext::Extension::builder("demo_hash")
        .add_capability(tabula_ext::Capability::new(descriptor.clone()))
        .build()
        .expect("build extension");

    let sdk = Sdk::builder()
        .with_extension(&extension)
        .expect("install extension")
        .build()
        .expect("build sdk");
    #[cfg(feature = "compile")]
    let artifact = sdk
        .compile(CAPABILITY_SOURCE)
        .expect("compile capability-backed source");
    #[cfg(not(feature = "compile"))]
    let artifact = {
        let compiler_catalogs = CompilerCatalogs::standard()
            .with_capability_descriptor(descriptor)
            .expect("demo hash compiler catalog");
        let compiled = compile_program_source_with_catalogs(CAPABILITY_SOURCE, &compiler_catalogs)
            .expect("compile capability-backed source");
        register_compiled(&sdk, compiled).expect("register compiled program")
    };

    let manifest = &artifact
        .registered_program()
        .program()
        .capability_manifest
        .entries;
    assert_eq!(manifest.len(), 1);
    assert_eq!(manifest[0].symbol, "demo_hash");
}
