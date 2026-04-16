//! Integration tests for the external-beta CLI surface.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tabula-cli"))
}

fn run_ok(cwd: Option<&Path>, args: &[&str]) -> Output {
    let mut command = cli();
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_err(cwd: Option<&Path>, args: &[&str]) -> Output {
    let mut command = cli();
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.args(args).output().unwrap();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tabula-cli-tests-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn cli_manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn membership_program() -> PathBuf {
    cli_manifest_dir().join("../sdk/examples/programs/membership.tab")
}

#[test]
fn root_help_lists_beta_surface() {
    let output = run_ok(None, &["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("schema"));
    assert!(stdout.contains("query"));
    assert!(stdout.contains("state"));
    assert!(stdout.contains("context"));
    assert!(stdout.contains("batch"));
    if cfg!(feature = "verify") {
        assert!(stdout.contains("inspect-proof"));
    }
    if cfg!(feature = "prove") {
        assert!(stdout.contains("prove"));
    } else {
        assert!(!stdout.contains("prove"));
    }
    if cfg!(feature = "verify") {
        assert!(stdout.contains("verify"));
    } else {
        assert!(!stdout.contains("verify"));
    }
}

#[test]
fn schema_json_matches_for_source_and_artifact() {
    let dir = temp_dir("schema");
    let program = membership_program();
    let artifact = dir.join("membership.json");

    run_ok(
        None,
        &[
            "compile",
            program.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ],
    );

    let source_output = run_ok(None, &["schema", program.to_str().unwrap(), "--json"]);
    let artifact_output = run_ok(None, &["schema", artifact.to_str().unwrap(), "--json"]);

    let source_json: serde_json::Value = serde_json::from_slice(&source_output.stdout).unwrap();
    let artifact_json: serde_json::Value = serde_json::from_slice(&artifact_output.stdout).unwrap();
    assert_eq!(source_json, artifact_json);
}

#[test]
fn membership_query_and_execute_workflow_is_cli_complete() {
    let dir = temp_dir("membership");
    let program = membership_program();
    let state = dir.join("state.json");
    let context = dir.join("context.json");
    let batch = dir.join("batch.json");
    let state_after = dir.join("state_after.json");
    let report = dir.join("report.json");
    let receipt = dir.join("receipt.bin");

    run_ok(
        None,
        &[
            "state",
            "init",
            "--program",
            program.to_str().unwrap(),
            "--out",
            state.to_str().unwrap(),
        ],
    );
    run_ok(
        None,
        &[
            "state",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--key",
            "[1]",
            "members",
            "tier",
            "0",
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "init",
            "--program",
            program.to_str().unwrap(),
            "--out",
            context.to_str().unwrap(),
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "caller",
            "7",
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "epoch",
            "11",
        ],
    );
    run_ok(None, &["batch", "init", "--out", batch.to_str().unwrap()]);
    run_ok(
        None,
        &[
            "batch",
            "call",
            "--program",
            program.to_str().unwrap(),
            "--batch",
            batch.to_str().unwrap(),
            "approve_upgrade",
            "--args",
            "[1]",
        ],
    );

    let query_output = run_ok(
        None,
        &[
            "query",
            "preview_upgrade",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "--args",
            "[1]",
            "--json",
        ],
    );
    let query_json: serde_json::Value = serde_json::from_slice(&query_output.stdout).unwrap();
    assert_eq!(
        query_json["returns"][0],
        serde_json::json!({"kind":"u64","value":1})
    );

    let execute_output = run_ok(
        None,
        &[
            "execute",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--batch",
            batch.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "--state-out",
            state_after.to_str().unwrap(),
            "--report-out",
            report.to_str().unwrap(),
            "--receipt-out",
            receipt.to_str().unwrap(),
        ],
    );
    let stdout = String::from_utf8_lossy(&execute_output.stdout);
    assert!(stdout.contains("approve_upgrade"));
    assert!(stdout.contains("members[1].tier = 1"));

    let inspect_output = run_ok(
        None,
        &[
            "state",
            "inspect",
            "--state",
            state_after.to_str().unwrap(),
            "--program",
            program.to_str().unwrap(),
        ],
    );
    assert!(String::from_utf8_lossy(&inspect_output.stdout).contains("members[1].tier = 1"));

    let report_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report_json["version"], "tabula.cli.execute.v1");
    assert!(receipt.is_file());
}

#[cfg(feature = "prove")]
#[test]
fn membership_prove_and_verify_round_trip() {
    let dir = temp_dir("membership-proof");
    let program = membership_program();
    let state = dir.join("state.json");
    let context = dir.join("context.json");
    let batch = dir.join("batch.json");
    let receipt = dir.join("receipt.bin");
    let proof = dir.join("proof.bin");
    let public_statement = dir.join("public_statement.json");
    let summary = dir.join("proof_summary.json");

    run_ok(
        None,
        &[
            "state",
            "init",
            "--program",
            program.to_str().unwrap(),
            "--out",
            state.to_str().unwrap(),
        ],
    );
    run_ok(
        None,
        &[
            "state",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--key",
            "[1]",
            "members",
            "tier",
            "0",
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "init",
            "--program",
            program.to_str().unwrap(),
            "--out",
            context.to_str().unwrap(),
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "caller",
            "7",
        ],
    );
    run_ok(
        None,
        &[
            "context",
            "set",
            "--program",
            program.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "epoch",
            "11",
        ],
    );
    run_ok(None, &["batch", "init", "--out", batch.to_str().unwrap()]);
    run_ok(
        None,
        &[
            "batch",
            "call",
            "--program",
            program.to_str().unwrap(),
            "--batch",
            batch.to_str().unwrap(),
            "approve_upgrade",
            "--args",
            "[1]",
        ],
    );
    run_ok(
        None,
        &[
            "execute",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--batch",
            batch.to_str().unwrap(),
            "--context",
            context.to_str().unwrap(),
            "--receipt-out",
            receipt.to_str().unwrap(),
        ],
    );

    let prove = run_ok(
        None,
        &[
            "prove",
            "--program",
            program.to_str().unwrap(),
            "--receipt",
            receipt.to_str().unwrap(),
            "--proof-out",
            proof.to_str().unwrap(),
            "--public-statement-out",
            public_statement.to_str().unwrap(),
            "--summary-out",
            summary.to_str().unwrap(),
            "--json",
        ],
    );
    let prove_json: serde_json::Value = serde_json::from_slice(&prove.stdout).unwrap();
    assert_eq!(prove_json["version"], "tabula.cli.prove.v1");
    assert!(proof.is_file());
    assert!(public_statement.is_file());
    assert!(summary.is_file());
    let public_statement_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&public_statement).unwrap()).unwrap();
    assert_eq!(
        public_statement_json["version"],
        serde_json::json!(tabula_sdk::PublicStatementFile::VERSION)
    );

    let verify = run_ok(
        None,
        &[
            "verify",
            "--program",
            program.to_str().unwrap(),
            "--proof",
            proof.to_str().unwrap(),
            "--statement",
            public_statement.to_str().unwrap(),
            "--json",
        ],
    );
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_json["version"], "tabula.cli.verify.v1");
    assert_eq!(verify_json["verified"], true);
}

#[test]
fn malformed_json_error_includes_the_offending_path() {
    let dir = temp_dir("diagnostics");
    let program = membership_program();
    let state = dir.join("state.json");
    let batch = dir.join("batch.json");

    run_ok(
        None,
        &[
            "state",
            "init",
            "--program",
            program.to_str().unwrap(),
            "--out",
            state.to_str().unwrap(),
        ],
    );
    std::fs::write(&batch, "{not valid json").unwrap();

    let output = run_err(
        None,
        &[
            "execute",
            "--program",
            program.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--batch",
            batch.to_str().unwrap(),
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(batch.to_str().unwrap()));
    assert!(stderr.contains("failed to parse JSON file"));
}
