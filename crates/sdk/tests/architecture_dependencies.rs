#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn read_workspace_file(rel: &str) -> String {
    fs::read_to_string(workspace_root().join(rel)).expect("read workspace file")
}

#[test]
fn sdk_root_keeps_raw_runtime_and_compiler_types_under_advanced_only() {
    let sdk_lib = read_workspace_file("crates/sdk/src/lib.rs");

    for required in [
        "pub mod interop;",
        "pub use environment::Environment;",
        "pub use builder::SdkBuilder;",
        "pub use sdk::Sdk;",
        "pub use types::{Context, State, TransactionBatch};",
        "pub use program::",
    ] {
        assert!(
            sdk_lib.contains(required),
            "crates/sdk/src/lib.rs must contain `{required}`"
        );
    }

    for forbidden in [
        "RegisteredProgram",
        "CompiledProgram",
        "CompilerCatalogs",
        "HostEnvironment",
        "ContextInput",
        "EntryBatch",
        "EntryCall",
        "StateSnapshot",
        "PortableValue",
        "EntryId",
        "FieldId",
        "TableId",
    ] {
        assert!(
            !sdk_lib.contains(forbidden),
            "crates/sdk/src/lib.rs must keep `{forbidden}` out of the default root surface"
        );
    }
}

#[test]
fn ext_root_keeps_authoring_surface_centered_on_extension_bundles() {
    let ext_lib = read_workspace_file("crates/ext/src/lib.rs");

    for required in [
        "mod extension;",
        "pub mod backend;",
        "pub mod prelude {",
        "pub use extension::{",
    ] {
        assert!(
            ext_lib.contains(required),
            "crates/ext/src/lib.rs must contain `{required}`"
        );
    }

    for forbidden in ["pub use scheme::", "pub use backend::"] {
        assert!(
            !ext_lib.contains(forbidden),
            "crates/ext/src/lib.rs must not flatten expert-only surface `{forbidden}` into the root"
        );
    }
}

#[test]
fn removed_sdk_compatibility_files_stay_deleted() {
    for rel in ["crates/sdk/src/execution.rs", "crates/sdk/src/ext/mod.rs"] {
        assert!(
            !workspace_root().join(rel).exists(),
            "{rel} must stay deleted in the final SDK surface"
        );
    }
}
