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

/// SP-3 §2.5 guardrail — enforces that execution-tier witness sources
/// reference `tabula_chips::` only through the allowlisted vocabulary,
/// and never by bare row-type tail (catching re-exports and inline
/// fully-qualified paths that would sidestep a `use`-line-only scanner).
///
/// Every file under `crates/witness/src/` must fall into exactly one
/// bucket:
///
/// - `DEFERRED_TIER_PREFIXES` — column/root-tier subtrees (SP-3 §9
///   non-goals). Skipped; will migrate in a future spike.
/// - everything else — scanned strictly. A file landing in a brand-new
///   subdirectory is scanned by default; relaxing it requires adding
///   the path to `DEFERRED_TIER_PREFIXES` with an explicit rationale.
///
/// Allowed path tails (SP-3 §2.5):
/// - protocol-level identifiers: `Opcode`, `CmpOp`, `MAX_SLOTS`,
///   `EXECUTION_STANDARD_VALUE_WIDTH`,
/// - witness-store label constants: any `*_WITNESS_LABEL`,
/// - crypto helpers: `native_key_payload_prefix3`, `poseidon2_permutation`,
/// - core row types: `InstructionRecord`, `StaticTableRow`,
/// - shared helpers: `EntrySource`,
/// - the concrete kit type set (witness ops files import the kit, never
///   the row type).
/// Subtrees skipped by the guardrail. These tiers intentionally
/// stay chip-aware per SP-3 §9 non-goals. Expanding this list
/// requires explicit justification.
const SP3_DEFERRED_TIER_PREFIXES: &[&str] = &[
    "crates/witness/src/stark/memory",
    "crates/witness/src/stark/roots",
    "crates/witness/src/stark/schemes",
];

/// Concrete set of kit types witness ops files may name. Replaces
/// the looser `ends_with("Kit")` rule so a hypothetical chip row
/// type named `FooKit` would still be rejected.
const SP3_ALLOWED_KITS: &[&str] = &[
    "IrHashKit",
    "RelationTranscriptKit",
    "RelationTableKit",
    "PublicContextTranscriptKit",
    "TxBatchTranscriptKit",
    "EventTranscriptKit",
];

fn sp3_last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

fn sp3_allowed(tail: &str) -> bool {
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
        || SP3_ALLOWED_KITS.contains(&tail)
}

fn sp3_scan_use_stmt(stmt: &str, path: &Path, forbidden: &mut Vec<(PathBuf, String)>) {
    // `stmt` is a single logical `use tabula_chips::...;` statement,
    // newlines already collapsed to spaces by the caller.
    let Some(rest) = stmt.trim_start().strip_prefix("use tabula_chips::") else {
        return;
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
            let tail = sp3_last_segment(head);
            if !sp3_allowed(tail) {
                forbidden.push((
                    path.to_path_buf(),
                    format!("tabula_chips::{prefix}{head}"),
                ));
            }
        }
    } else {
        let head = body.split(" as ").next().unwrap_or(body).trim();
        let tail = sp3_last_segment(head);
        if !sp3_allowed(tail) {
            forbidden.push((path.to_path_buf(), format!("tabula_chips::{head}")));
        }
    }
}

/// Also look for `tabula_chips::…::Ident` occurrences outside a
/// `use` statement — inline fully-qualified references and
/// re-exports from other crates that pass a forbidden tail through.
fn sp3_scan_inline_fqn(line: &str, path: &Path, forbidden: &mut Vec<(PathBuf, String)>) {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return;
    }
    let mut rest = line;
    while let Some(at) = rest.find("tabula_chips::") {
        rest = &rest[at + "tabula_chips::".len()..];
        // Read the Rust path that follows: segments of [A-Za-z0-9_]
        // joined by `::`. Stop at the first non-matching char.
        let mut end = 0;
        let bytes = rest.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            let ident = c.is_ascii_alphanumeric() || c == b'_';
            let sep = i + 1 < bytes.len()
                && c == b':'
                && bytes[i + 1] == b':'
                && i > 0
                && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if ident {
                end = i + 1;
                i += 1;
            } else if sep {
                end = i + 2;
                i += 2;
            } else {
                break;
            }
        }
        if end == 0 {
            continue;
        }
        let path_seg = &rest[..end];
        let tail = sp3_last_segment(path_seg);
        if !sp3_allowed(tail) {
            forbidden.push((
                path.to_path_buf(),
                format!("tabula_chips::{path_seg} (inline FQN)"),
            ));
        }
        rest = &rest[end..];
    }
}

/// Scan one source string for forbidden `tabula_chips::…` references.
///
/// Multi-line grouped `use` statements are folded into a single
/// logical statement before scanning so a form like
/// `use tabula_chips::{\n    ir_hash::IrHashCall,\n};` is covered.
/// Inline FQN references are scanned line-by-line, but lines that
/// belong to the body of a `use tabula_chips::` statement are
/// excluded so their continuation fragments are not double-flagged.
fn sp3_collect_chip_refs(source: &str, path: &Path, forbidden: &mut Vec<(PathBuf, String)>) {
    let mut buffer: Option<String> = None;
    let mut in_use_block = false;
    for line in source.lines() {
        if let Some(buf) = buffer.as_mut() {
            buf.push(' ');
            buf.push_str(line.trim());
            if line.contains(';') {
                sp3_scan_use_stmt(buf, path, forbidden);
                buffer = None;
                in_use_block = false;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("use tabula_chips::") {
            if line.contains(';') {
                sp3_scan_use_stmt(line, path, forbidden);
            } else {
                buffer = Some(trimmed.to_string());
                in_use_block = true;
            }
            continue;
        }
        if !in_use_block {
            sp3_scan_inline_fqn(line, path, forbidden);
        }
    }
}

#[test]
fn sp3_guardrail_detects_multiline_grouped_chip_imports() {
    // Synthetic source containing a multi-line grouped `use` with a
    // forbidden tail. The per-line scanner the guardrail started with
    // would pass this through; the folded scanner must flag it.
    let src = "use tabula_chips::{\n    ir_hash::IrHashCall,\n};\n";
    let mut forbidden = Vec::new();
    sp3_collect_chip_refs(src, Path::new("synthetic.rs"), &mut forbidden);
    assert!(
        !forbidden.is_empty(),
        "multi-line grouped chip import must be flagged",
    );
}

#[test]
fn sp3_guardrail_accepts_multiline_allowed_imports() {
    // Multi-line grouped imports for allowed kits must NOT be flagged.
    let src = "use tabula_chips::{\n    ir_hash::IrHashKit,\n    relation_table::RelationTableKit,\n};\n";
    let mut forbidden = Vec::new();
    sp3_collect_chip_refs(src, Path::new("synthetic.rs"), &mut forbidden);
    assert!(
        forbidden.is_empty(),
        "allowed kits in a multi-line group must pass: {forbidden:?}",
    );
}

#[test]
fn sp3_witness_chip_import_guardrail() {
    let sources = rust_sources_under("crates/witness/src");
    let mut forbidden: Vec<(PathBuf, String)> = Vec::new();

    for path in sources {
        let path_str = path.to_string_lossy();
        if SP3_DEFERRED_TIER_PREFIXES
            .iter()
            .any(|prefix| path_str.contains(prefix))
        {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read witness source");
        sp3_collect_chip_refs(&source, &path, &mut forbidden);
    }

    assert!(
        forbidden.is_empty(),
        "SP-3 guardrail: {} forbidden chip reference(s) under tabula-witness execution-tier surface:\n{}",
        forbidden.len(),
        forbidden
            .iter()
            .map(|(path, import)| format!("  {}: {}", path.display(), import))
            .collect::<Vec<_>>()
            .join("\n"),
    );
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
