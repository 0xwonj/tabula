use tabula_core::TypeId;
use tabula_lang::hir::{
    CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality, HashFamily,
};
use tabula_lang::{CapabilityPreludeEntry, FrontendErrorKind, FrontendPrelude};
use tabula_profile::{
    GenericIrFamily, HostValueFamily, NullSemantics, TYPE_BYTES32_ID, TYPE_U64_ID,
    TypeCapabilities, TypeDescriptor, ZeroValueSpec, builtin_semantic_registry,
};

#[allow(dead_code)]
pub fn prelude() -> FrontendPrelude {
    FrontendPrelude::new(
        tabula_profile::builtin_semantic_registry().expect("registry"),
        vec![CapabilityPreludeEntry {
            path: "poseidon_hash".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_BYTES32_ID],
            totality: CapabilityTotality::Total,
            query_policy: CapabilityQueryPolicy::QuerySafe,
            proof_visibility: CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: Some(HashFamily::Poseidon),
        }],
    )
    .expect("prelude")
}

#[allow(dead_code)]
pub fn custom_no_eq_prelude() -> FrontendPrelude {
    let mut registry = builtin_semantic_registry().expect("registry");
    registry
        .register_type_descriptor(
            TypeDescriptor::new(
                TypeId(9000),
                "OpaqueNoEq",
                None,
                HostValueFamily::Opaque {
                    family: "opaque_no_eq".into(),
                },
                GenericIrFamily::EqOnly,
                TypeCapabilities {
                    equality: false,
                    ordering: false,
                    arithmetic: false,
                },
                ZeroValueSpec::ZeroBytes { len: 32 },
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("type descriptor"),
        )
        .expect("register type descriptor");
    registry
        .register_type_name("OpaqueNoEq", TypeId(9000))
        .expect("type name");
    FrontendPrelude::new(registry, vec![]).expect("prelude")
}

#[allow(dead_code)]
pub fn assert_unsupported(source: &str, needle: &str) {
    let err = tabula_lang::parse_program(source).expect_err("parse should fail");
    assert_eq!(err.kind, FrontendErrorKind::UnsupportedFeature);
    assert!(
        err.message.contains(needle),
        "expected {:?} in {:?}",
        needle,
        err.message
    );
}
