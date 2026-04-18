//! Final architecture guardrails for the native runtime surface.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace_root())
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

fn direct_normal_deps(metadata: &Value, package_name: &str) -> Vec<String> {
    metadata["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .find(|pkg| pkg["name"].as_str() == Some(package_name))
        .unwrap_or_else(|| panic!("package '{package_name}' missing from cargo metadata"))
        ["dependencies"]
        .as_array()
        .expect("dependency array")
        .iter()
        .filter(|dep| dep["kind"].is_null())
        .map(|dep| dep["name"].as_str().expect("dependency name").to_string())
        .collect()
}

fn assert_forbidden_dep(metadata: &Value, package_name: &str, forbidden: &[&str]) {
    let deps = direct_normal_deps(metadata, package_name);
    for blocked in forbidden {
        assert!(
            !deps.iter().any(|dep| dep == blocked),
            "{package_name} must not depend on {blocked}: {deps:?}"
        );
    }
}

fn read_workspace_file(rel: &str) -> String {
    fs::read_to_string(workspace_root().join(rel)).expect("read workspace file")
}

fn rust_sources_under(rel: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(&workspace_root().join(rel), &mut files);
    files.sort();
    files
}

fn leaked(parts: &[&str]) -> &'static str {
    Box::leak(parts.concat().into_boxed_str())
}

fn assert_source_omits(rel: &str, forbidden: &[&str]) {
    let source = read_workspace_file(rel);
    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "{rel} must not contain forbidden pattern '{needle}'"
        );
    }
}

fn assert_source_prefix_omits(rel: &str, split_marker: &str, forbidden: &[&str]) {
    let source = read_workspace_file(rel);
    let prefix = source.split(split_marker).next().unwrap_or(source.as_str());
    for needle in forbidden {
        assert!(
            !prefix.contains(needle),
            "{rel} production source must not contain forbidden pattern '{needle}' before '{split_marker}'"
        );
    }
}

#[test]
fn runtime_and_machine_boundary_packages_drop_legacy_program_carriers() {
    let metadata = cargo_metadata();

    assert_forbidden_dep(&metadata, "tabula-runtime", &["tabula-artifact"]);
    assert_forbidden_dep(&metadata, "tabula-compiler", &["tabula-artifact"]);
    assert_forbidden_dep(&metadata, "tabula-machine", &["tabula-runtime"]);
}

#[test]
fn runtime_root_exposes_only_the_final_native_surface() {
    let runtime_lib = read_workspace_file("crates/runtime/src/lib.rs");

    assert!(
        runtime_lib.contains("#[cfg(feature = \"verify\")]\nmod host;"),
        "runtime host surface must stay gated to the verify surface"
    );
    assert!(
        runtime_lib.contains("#[cfg(feature = \"verify\")]\nmod verifier;"),
        "runtime verifier surface must live in its dedicated module"
    );
    assert!(
        runtime_lib.contains("pub mod semantics;"),
        "runtime root must expose semantic helpers"
    );
    assert!(
        runtime_lib.contains(
            "pub use engine::{CommittedStateSnapshot, ExecutionReceipt, RuntimeBuilder, TabulaRuntime};"
        ) && runtime_lib.contains("pub use tabula_contract::{BoundStatement, PublicStatement};")
            && runtime_lib.contains("pub use engine::{ProveInput, ProveResult, VerifiedResult};")
            && runtime_lib.contains("pub use verifier::{Verifier, VerifierBuilder};"),
        "runtime root must re-export the canonical native runtime and verifier types"
    );
    for forbidden in [
        leaked(&["pub mod ", "next;"]),
        "pub type ProgramVerifier",
        "pub type RuntimeProgram",
        leaked(&["tabula_", "artifact"]),
        leaked(&["Sealed", "Program"]),
    ] {
        assert!(
            !runtime_lib.contains(forbidden),
            "runtime root must not expose removed compatibility surface '{forbidden}'"
        );
    }
}

#[test]
fn live_runtime_sources_are_legacy_free() {
    let compiled_paths = [
        "crates/runtime/src/lib.rs",
        "crates/runtime/src/error.rs",
        "crates/runtime/src/semantics.rs",
        "crates/runtime/src/engine.rs",
        "crates/runtime/src/verifier.rs",
        "crates/runtime/src/state_runtime.rs",
        "crates/runtime/src/proof_summary.rs",
        "crates/runtime/src/bootstrap/mod.rs",
        "crates/runtime/src/bootstrap/machine.rs",
        "crates/runtime/src/bootstrap/program.rs",
    ];

    for rel in compiled_paths {
        assert_source_omits(
            rel,
            &[
                leaked(&["tabula_", "artifact", "::"]),
                leaked(&["tabula_compiler::", "Sealed", "Program"]),
                leaked(&["legacy", "::"]),
                leaked(&["tabula_runtime::", "next"]),
            ],
        );
    }

    for path in rust_sources_under("crates/runtime/src/host") {
        let source = fs::read_to_string(&path).expect("read runtime host source");
        for needle in [
            leaked(&["tabula_", "artifact", "::"]),
            leaked(&["tabula_compiler::", "Sealed", "Program"]),
            leaked(&["legacy", "::"]),
        ] {
            assert!(
                !source.contains(needle),
                "{} must not contain forbidden runtime compatibility pattern '{}'",
                path.display(),
                needle
            );
        }
    }
}

#[test]
fn runtime_state_bootstrap_uses_sealed_column_contracts_directly() {
    assert_source_omits(
        "crates/runtime/src/state_runtime.rs",
        &["resolve_field_profile("],
    );
}

#[test]
fn native_proof_path_stays_bridge_free() {
    assert_source_omits(
        "crates/runtime/src/engine.rs",
        &[
            leaked(&["tabula_", "artifact", "::"]),
            leaked(&["tabula_compiler::", "Sealed", "Program"]),
            "prove_query(",
            leaked(&["legacy", "::"]),
        ],
    );
    assert_source_omits(
        "crates/runtime/src/engine.rs",
        &[
            "struct VerifierCore",
            "pub struct VerifierBuilder",
            "pub struct Verifier {",
            "fn validate_core_first_program(",
            "fn materialize_registered_state_runtime(",
            "fn program_uses_hash(",
            "fn program_uses_relations(",
        ],
    );
    assert_source_omits(
        "crates/witness/src/stark/lowering/driver.rs",
        &[
            leaked(&["tabula_", "artifact", "::"]),
            "tabula_ir::TxTypeDef",
            leaked(&["legacy", "::"]),
        ],
    );
}

#[test]
fn verifier_path_is_single_sourced_in_verifier_module() {
    let verifier_source = read_workspace_file("crates/runtime/src/verifier.rs");
    assert!(
        verifier_source.contains("struct VerifierCore")
            && verifier_source.contains("pub struct VerifierBuilder")
            && verifier_source.contains("pub struct Verifier"),
        "runtime verifier module must own the canonical verification path"
    );
    assert!(
        !verifier_source.contains("crate::engine::"),
        "runtime verifier module must not depend on proving orchestration in engine.rs"
    );
}

#[test]
fn runtime_relation_proof_prep_stays_witness_owned() {
    // `RelationTableWitnessRow` intentionally dropped: SP-3 S3.2 migrated
    // the relation-table witness row construction out of `tabula-witness`
    // (so witness stays chip-agnostic) into the runtime call site that
    // already drives the chip-kit scratchpad pre-stuff. The actual relation
    // proof preparation (`prepare_relation_proof`, transcript digests)
    // still lives in witness — that is what the rest of this guardrail
    // continues to protect.
    assert_source_prefix_omits(
        "crates/runtime/src/engine.rs",
        "#[cfg(all(test, feature = \"prove\"))]",
        &[
            "RelationTranscriptCall",
            "compute_typed_tuple_digest",
            "typed_tuple_transcript",
            "relation_transcript::",
        ],
    );
}

/// Guardrail: every production call site that drives the STARK machine must go
/// through [`tabula_machine::BackendProver`] / [`tabula_machine::BackendVerifier`]
/// rather than the internal `TabulaMachine::prove` / `verify` helpers.
///
/// Scope is intentionally limited to `crates/runtime/src/{engine,verifier}.rs`
/// because the runtime is the only workspace crate that may construct a
/// `TabulaMachine` and drive it directly. `tabula-sdk` reaches the backend
/// exclusively through `tabula_runtime::TabulaRuntime::prove` and through
/// `Verifier` — it does not depend on `tabula-machine` except for re-exported
/// envelope types. If that changes (e.g. SDK gains a direct `TabulaMachine`
/// handle), extend `facade_routing_paths` below to cover `crates/sdk/src/**`.
#[test]
fn runtime_prove_and_verify_route_through_backend_facade() {
    let engine = read_workspace_file("crates/runtime/src/engine.rs");
    let engine_prod = engine
        .split("#[cfg(all(test, feature = \"prove\"))]")
        .next()
        .unwrap_or(engine.as_str());
    assert!(
        engine_prod.contains("BackendProver::new(&self.machine)")
            && engine_prod.contains(".prove_envelope("),
        "runtime prove path must go through BackendProver::prove_envelope"
    );
    for forbidden in [
        leaked(&["self.machine.", "prove("]),
        leaked(&["self.machine.", "verify("]),
    ] {
        assert!(
            !engine_prod.contains(forbidden),
            "runtime engine production path must not call '{forbidden}' directly — route through the BackendProver/BackendVerifier facade"
        );
    }

    let verifier = read_workspace_file("crates/runtime/src/verifier.rs");
    assert!(
        verifier.contains("BackendVerifier::new(self.machine)")
            && verifier.contains(".verify_proof("),
        "runtime verifier path must go through BackendVerifier::verify_proof"
    );
    for forbidden in [
        leaked(&["self.machine.", "verify("]),
        leaked(&["self.machine.", "prove("]),
    ] {
        assert!(
            !verifier.contains(forbidden),
            "runtime verifier must not call '{forbidden}' directly — route through the BackendVerifier facade"
        );
    }
}

#[test]
fn machine_input_uses_explicit_air_and_semantic_statement_names() {
    let machine_input = read_workspace_file("crates/machine/src/input/mod.rs");

    assert!(
        machine_input.contains("pub binding_digest: [u8; 32]"),
        "machine input must expose an explicit binding-digest field"
    );
    for forbidden in [
        "pub public_statement: PublicStatement",
        "pub air_statement: PublicStatement",
        "pub semantic_statement_digest: [u8; 32]",
    ] {
        assert!(
            !machine_input.contains(forbidden),
            "machine input must not carry a statement on the backend primitive: '{forbidden}'"
        );
    }
}

#[test]
fn removed_runtime_compatibility_tree_stays_deleted() {
    for rel in [
        "crates/runtime/src/bootstrap/builder.rs",
        "crates/runtime/src/bootstrap/materialize.rs",
        "crates/runtime/src/bootstrap/registries.rs",
        "crates/runtime/src/bootstrap/validation.rs",
        "crates/runtime/src/execute",
        "crates/runtime/src/policy",
        "crates/runtime/src/program",
        "crates/runtime/src/proving",
        "crates/runtime/src/runtime.rs",
        "crates/runtime/src/testing",
    ] {
        assert!(
            !workspace_root().join(rel).exists(),
            "{rel} must remain deleted in the final native runtime surface"
        );
    }
}

fn markdown_sources_under(rel: &str, skipped_dirs: &[&str]) -> Vec<PathBuf> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>, skipped_dirs: &[&str]) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| skipped_dirs.contains(&name));
                if !skip {
                    walk(&path, files, skipped_dirs);
                }
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(&workspace_root().join(rel), &mut files, skipped_dirs);
    files.sort();
    files
}

fn crate_readmes_under(rel: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("README.md") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(&workspace_root().join(rel), &mut files);
    files.sort();
    files
}

/// SP-3 §2.5 guardrail — **off mode** (S1): scans `crates/witness/src/**/*.rs`
/// for `use tabula_chips::...` paths and reports anything outside the
/// allowlist without failing. S4 flips this to assertive.
///
/// Allowlist (from SP-3 §2.5):
/// - protocol-level identifiers: `Opcode`, `CmpOp`, `MAX_SLOTS`,
///   `EXECUTION_STANDARD_VALUE_WIDTH`,
/// - witness-store label constants: any `*_WITNESS_LABEL`,
/// - crypto helpers: `native_key_payload_prefix3`, `poseidon2_permutation`,
/// - core row types: `InstructionRecord`, `StaticTableRow`,
/// - shared helpers: `EntrySource`,
/// - `*Kit` types (post-S2 — witness ops files import the kit, never
///   the row type).
#[test]
fn sp3_witness_chip_import_guardrail_off_mode() {
    fn last_segment(path: &str) -> &str {
        path.rsplit("::").next().unwrap_or(path)
    }

    fn allowed(tail: &str) -> bool {
        matches!(
            tail,
            "Opcode"
                | "CmpOp"
                | "MAX_SLOTS"
                | "EXECUTION_STANDARD_VALUE_WIDTH"
                | "native_key_payload_prefix3"
                | "poseidon2_permutation"
                | "InstructionRecord"
                | "StaticTableRow"
                | "EntrySource"
        ) || tail.ends_with("_WITNESS_LABEL")
            || tail.ends_with("Kit")
    }

    let sources = rust_sources_under("crates/witness/src");
    let mut forbidden: Vec<(PathBuf, String)> = Vec::new();

    for path in sources {
        let source = fs::read_to_string(&path).expect("read witness source");
        for line in source.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("use tabula_chips::") else {
                continue;
            };
            let body = rest.trim_end_matches(';').trim();
            if let Some(open) = body.find('{') {
                let prefix = &body[..open];
                let close = body.rfind('}').unwrap_or(body.len());
                let group = &body[open + 1..close];
                for item in group.split(',') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    let head = item.split(" as ").next().unwrap_or(item).trim();
                    let tail = last_segment(head);
                    if !allowed(tail) {
                        forbidden.push((path.clone(), format!("tabula_chips::{prefix}{head}")));
                    }
                }
            } else {
                let head = body.split(" as ").next().unwrap_or(body).trim();
                let tail = last_segment(head);
                if !allowed(tail) {
                    forbidden.push((path.clone(), format!("tabula_chips::{head}")));
                }
            }
        }
    }

    if !forbidden.is_empty() {
        eprintln!(
            "[SP-3 off-mode] {} chip-row import(s) still live in tabula-witness (pending migration to ChipWitnessKit):",
            forbidden.len()
        );
        for (path, import) in &forbidden {
            eprintln!("  {}: {}", path.display(), import);
        }
    }
}

#[test]
fn live_sources_do_not_reintroduce_legacy_capability_vocabulary() {
    let mut files = rust_sources_under("crates");
    files.extend(crate_readmes_under("crates"));
    files.extend(markdown_sources_under("docs/design", &[]));
    files.sort();
    files.dedup();

    for path in files {
        let source = fs::read_to_string(&path).expect("read live source");
        for needle in [
            leaked(&["pre", "compile"]),
            leaked(&["Pre", "compile"]),
            leaked(&["PRE", "COMPILE"]),
        ] {
            assert!(
                !source.contains(needle),
                "{} must not contain legacy capability vocabulary '{}'",
                path.display(),
                needle
            );
        }
    }
}
