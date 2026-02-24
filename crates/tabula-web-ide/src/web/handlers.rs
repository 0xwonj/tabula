//! Async handler factory functions for the App component.
//!
//! Each handler receives `AppSignals` (which is `Copy`) and returns a closure
//! suitable for use in Leptos event bindings.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_file::{File, callbacks::FileReader, callbacks::read_as_text};
use leptos::prelude::*;
use serde_json::json;
use tabula_core::Value as CoreValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use crate::web::api::ApiClient;
use crate::web::app_state::AppSignals;
use crate::web::models::{BatchFile, ProgramArtifact, StateCell, StateFile, VerifyReport};
use crate::web::templates::template_workspace;
use crate::web::utils::{
    default_value_for_type, format_api_err, opt_token, parse_batch, parse_state, pretty_json_value,
};

// ── Connect daemon ──────────────────────────────────────────────────

pub(crate) fn connect_daemon(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        if s.busy_action.get().is_some() {
            return;
        }

        let base = s.daemon_url.get();
        let token = s.auth_token.get();
        let client = ApiClient::new(base.clone(), opt_token(token));

        s.set_busy_action.set(Some("connect".to_string()));
        s.set_status_line.set(format!("Connecting to {base} ..."));

        spawn_local(async move {
            let health_res = client.health().await;
            let caps_res = client.capabilities().await;

            match (health_res, caps_res) {
                (Ok(health_ok), Ok(caps_ok)) => {
                    s.set_health.set(Some(health_ok));
                    s.set_capabilities_json
                        .set(pretty_json_value(&json!(caps_ok)));
                    s.set_diagnostics_text
                        .set("Connected. Health/capabilities synced.".to_string());
                    s.set_status_line.set("Daemon connected".to_string());
                    s.append_history("connect", true, "health + capabilities ok".to_string());
                }
                (Err(e), _) | (_, Err(e)) => {
                    s.set_status_line
                        .set(format!("Connection failed: {}", e.message));
                    s.set_diagnostics_text.set(format_api_err("connect", &e));
                    s.append_history("connect", false, e.message);
                }
            }

            s.set_busy_action.set(None);
        });
    }
}

// ── Check program ───────────────────────────────────────────────────

pub(crate) fn run_check(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        if s.busy_action.get().is_some() {
            return;
        }

        s.clear_verify_gate();
        let base = s.daemon_url.get();
        let token = s.auth_token.get();
        let source = s.program_source.get();
        let client = ApiClient::new(base, opt_token(token));

        s.set_busy_action.set(Some("check".to_string()));
        s.set_active_tab.set("diagnostics".to_string());
        s.set_status_line.set("Running check ...".to_string());

        spawn_local(async move {
            match client.register_program(&source).await {
                Ok(resp) => {
                    s.set_diagnostics_text.set(format!(
                        "CHECK OK (register)\n- program_id: {}\n- table_count: {}\n- tx_type_count: {}",
                        resp.program.program_id,
                        resp.program.table_count,
                        resp.program.tx_type_count,
                    ));
                    s.set_status_line.set("Check finished".to_string());
                    s.append_history(
                        "check",
                        true,
                        format!(
                            "{} table(s), {} tx type(s)",
                            resp.program.table_count, resp.program.tx_type_count
                        ),
                    );
                }
                Err(e) => {
                    s.set_diagnostics_text.set(format_api_err("check", &e));
                    s.set_status_line
                        .set(format!("Check failed: {}", e.message));
                    s.append_history("check", false, e.message);
                }
            }

            s.set_busy_action.set(None);
        });
    }
}

// ── Deploy (compile + register + create instance) ───────────────────

pub(crate) fn run_deploy(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        if s.busy_action.get().is_some() {
            return;
        }

        s.clear_verify_gate();

        let state = match parse_state(&s.state_json.get()) {
            Ok(state) => state,
            Err(e) => {
                s.set_diagnostics_text
                    .set(format!("STATE JSON ERROR: {e}"));
                s.append_history("deploy", false, format!("state parse failed: {e}"));
                return;
            }
        };

        let base = s.daemon_url.get();
        let token = s.auth_token.get();
        let source = s.program_source.get();
        let client = ApiClient::new(base, opt_token(token));

        s.set_busy_action.set(Some("deploy".to_string()));
        s.set_active_tab.set("diagnostics".to_string());
        s.set_status_line
            .set("Deploying (compile + register + create instance) ...".to_string());

        spawn_local(async move {
            match client.register_program(&source).await {
                Ok(program_resp) => {
                    s.set_compiled_ir_json
                        .set(pretty_json_value(&program_resp.program.program));
                    match client
                        .create_instance(&program_resp.program.program_id, state)
                        .await
                    {
                        Ok(instance_resp) => {
                            s.set_deployed_program_id
                                .set(Some(program_resp.program.program_id.clone()));
                            s.set_deployed_instance_id
                                .set(Some(instance_resp.instance.instance_id.clone()));
                            s.set_deployed_instance_version
                                .set(instance_resp.instance.version);

                            let artifact: Option<ProgramArtifact> =
                                serde_json::from_value(program_resp.program.program.clone()).ok();
                            s.set_program_artifact.set(artifact);

                            s.set_diagnostics_text.set(format!(
                                "DEPLOY OK\n- program_id: {}\n- instance_id: {}\n- version: {}\n- table_count: {}\n- tx_type_count: {}",
                                program_resp.program.program_id,
                                instance_resp.instance.instance_id,
                                instance_resp.instance.version,
                                program_resp.program.table_count,
                                program_resp.program.tx_type_count,
                            ));
                            s.set_status_line.set(format!(
                                "Deployed: instance={}",
                                instance_resp.instance.instance_id
                            ));
                            s.append_history(
                                "deploy",
                                true,
                                format!("instance_id={}", instance_resp.instance.instance_id),
                            );
                        }
                        Err(e) => {
                            s.set_diagnostics_text
                                .set(format_api_err("deploy:create_instance", &e));
                            s.set_status_line
                                .set(format!("Deploy failed: {}", e.message));
                            s.append_history("deploy", false, e.message);
                        }
                    }
                }
                Err(e) => {
                    s.set_diagnostics_text
                        .set(format_api_err("deploy:register_program", &e));
                    s.set_status_line
                        .set(format!("Deploy failed: {}", e.message));
                    s.append_history("deploy", false, e.message);
                }
            }

            s.set_busy_action.set(None);
        });
    }
}

// ── Submit batch ────────────────────────────────────────────────────

pub(crate) fn run_submit(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        if s.busy_action.get().is_some() {
            return;
        }

        let Some(instance_id) = s.deployed_instance_id.get() else {
            s.set_status_line
                .set("Submit blocked: deploy first.".to_string());
            s.append_history("submit", false, "no deployed instance".to_string());
            return;
        };

        let batch = match parse_batch(&s.batch_json.get()) {
            Ok(batch) => batch,
            Err(e) => {
                s.set_diagnostics_text
                    .set(format!("BATCH JSON ERROR: {e}"));
                s.append_history("submit", false, format!("batch parse failed: {e}"));
                return;
            }
        };

        let base = s.daemon_url.get();
        let token = s.auth_token.get();
        let include_trace_value = s.include_trace.get();
        let version = s.deployed_instance_version.get();
        let client = ApiClient::new(base, opt_token(token));

        s.set_busy_action.set(Some("submit".to_string()));
        s.set_active_tab.set("proof".to_string());
        s.set_status_line
            .set("Submitting batch (execute + prove + verify + commit) ...".to_string());

        spawn_local(async move {
            match client
                .submit_run(
                    &instance_id,
                    batch,
                    include_trace_value,
                    true,
                    true,
                    true,
                    Some(version),
                )
                .await
            {
                Ok(resp) => {
                    s.set_last_run_id.set(Some(resp.run.run_id.clone()));
                    s.set_deployed_instance_version.set(version + 1);

                    let state_after_str =
                        pretty_json_value(&json!(resp.run.execution.state_after));

                    let execution_blob = json!({
                        "tx_outcomes": resp.run.execution.tx_outcomes,
                        "consistency": resp.run.execution.consistency,
                        "emitted": resp.run.execution.emitted,
                        "read_set": resp.run.execution.read_set,
                        "write_set": resp.run.execution.write_set,
                    });
                    s.set_execution_json
                        .set(pretty_json_value(&execution_blob));

                    s.set_rw_diff_json.set(pretty_json_value(&json!({
                        "read_set": resp.run.execution.read_set,
                        "write_set": resp.run.execution.write_set,
                    })));

                    s.set_trace_json.set(pretty_json_value(&json!({
                        "trace": resp.run.execution.trace,
                    })));

                    if let Some(stark) = &resp.run.stark_proof {
                        let stark_display = pretty_json_value(&json!(stark));
                        s.set_proof_json.set(stark_display);
                        s.set_proof_log_json.set(format!(
                            "STARK PROOF\n- scheme: {}\n- verified: {}\n- chips: {}\n- prove: {}ms\n- verify: {}ms\n- old_root: {:?}\n- new_root: {:?}",
                            stark.scheme,
                            stark.verified,
                            stark.chip_count,
                            stark.prove_time_ms,
                            stark.verify_time_ms,
                            stark.old_state_root,
                            stark.new_state_root,
                        ));
                        s.set_stark_summary.set(Some(stark.clone()));

                        let report = VerifyReport {
                            ok: stark.verified,
                            mode: "stark_inline".to_string(),
                            message: if stark.verified {
                                "STARK proof verified".to_string()
                            } else {
                                "STARK proof verification failed".to_string()
                            },
                            statement_hash: Some(stark.statement_hash.clone()),
                            expected_statement_hash: None,
                            checked_at_ms: crate::web::storage::now_ms(),
                            raw: Some(json!(stark)),
                        };
                        s.set_verify_result_json
                            .set(pretty_json_value(&json!(report)));
                        s.set_verify_report.set(Some(report));
                    } else if let Some(proof) = &resp.run.proof {
                        let proof_json_text = pretty_json_value(&json!(proof));
                        s.set_proof_json.set(proof_json_text);
                        s.set_proof_log_json.set(format!(
                            "RECEIPT\n- scheme: {}\n- statement_hash: {}",
                            proof.scheme, proof.statement_hash
                        ));
                        s.set_stark_summary.set(None);
                    }

                    s.set_state_json.set(state_after_str);
                    s.set_pending_state_after.set(None);

                    s.set_status_line.set(format!(
                        "Submitted: run_id={}, v{}",
                        resp.run.run_id,
                        version + 1
                    ));
                    s.set_diagnostics_text.set(format!(
                        "SUBMIT OK\n- run_id: {}\n- status: {}\n- statement_hash: {}",
                        resp.run.run_id, resp.run.status, resp.run.statement_hash
                    ));
                    s.append_history("submit", true, format!("run_id={}", resp.run.run_id));
                }
                Err(e) => {
                    s.set_diagnostics_text.set(format_api_err("submit", &e));
                    s.set_status_line
                        .set(format!("Submit failed: {}", e.message));
                    s.append_history("submit", false, e.message);
                }
            }

            s.persist();
            s.set_busy_action.set(None);
        });
    }
}

// ── Load template ───────────────────────────────────────────────────

pub(crate) fn load_template(
    s: AppSignals,
) -> impl Fn(&'static str) + Clone + 'static {
    move |id: &'static str| {
        if let Some(ws) = template_workspace(id) {
            s.set_program_source.set(ws.program_source);
            s.set_state_json.set(ws.state_json);
            s.set_batch_json.set(ws.batch_json);
            s.set_pending_state_after.set(None);
            s.set_last_run_id.set(None);
            s.set_deployed_program_id.set(None);
            s.set_deployed_instance_id.set(None);
            s.set_deployed_instance_version.set(0);
            s.set_program_artifact.set(None);
            s.set_stark_summary.set(None);
            s.clear_verify_gate();
            s.set_status_line.set(format!("Template loaded: {id}"));
            s.append_history("template", true, format!("loaded {id}"));
            s.persist();
        }
    }
}

// ── Add state row ───────────────────────────────────────────────────

pub(crate) fn add_state_row(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        let artifact = s.program_artifact.get();
        let mut state = parse_state(&s.state_json.get()).unwrap_or(StateFile { cells: vec![] });

        if let Some(ref art) = artifact {
            if let Some(schema) = art.table_schemas.first() {
                let next_row = state
                    .cells
                    .iter()
                    .filter(|c| c.table == schema.id.0)
                    .map(|c| c.row)
                    .max()
                    .map(|r| r + 1)
                    .unwrap_or(0);
                for col_def in &schema.columns {
                    state.cells.push(StateCell {
                        table: schema.id.0,
                        row: next_row,
                        col: col_def.id.0,
                        value: Some(default_value_for_type(&format!("{:?}", col_def.value_type))),
                    });
                }
            }
        } else {
            state.cells.push(StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: Some(CoreValue::U64(0)),
            });
        }

        s.set_state_json.set(pretty_json_value(&json!(state)));
        s.persist();
    }
}

// ── Add tx row ──────────────────────────────────────────────────────

pub(crate) fn add_tx_row(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        let artifact = s.program_artifact.get();
        let mut batch = parse_batch(&s.batch_json.get()).unwrap_or(BatchFile {
            transactions: vec![],
        });

        let next_nonce = batch.transactions.len() as u64;

        if let Some(ref art) = artifact {
            if let Some(tx_def) = art.tx_types.first() {
                let params: Vec<CoreValue> = tx_def
                    .param_schema
                    .iter()
                    .map(|p| default_value_for_type(&format!("{:?}", p.value_type)))
                    .collect();
                batch
                    .transactions
                    .push(crate::web::models::TxInput {
                        tx_type: tx_def.id.0,
                        params,
                        sender: "01".repeat(32),
                        nonce: next_nonce,
                    });
            }
        } else {
            batch
                .transactions
                .push(crate::web::models::TxInput {
                    tx_type: 0,
                    params: vec![CoreValue::U64(0)],
                    sender: "01".repeat(32),
                    nonce: next_nonce,
                });
        }

        s.set_batch_json.set(pretty_json_value(&json!(batch)));
        s.persist();
    }
}

// ── Export workspace ────────────────────────────────────────────────

pub(crate) fn export_workspace(
    s: AppSignals,
) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        let doc = crate::web::models::WorkspaceDoc {
            daemon_url: s.daemon_url.get(),
            auth_token: s.auth_token.get(),
            program_source: s.program_source.get(),
            state_json: s.state_json.get(),
            batch_json: s.batch_json.get(),
            include_trace: s.include_trace.get(),
            proof_json: s.proof_json.get(),
            verify_result_json: s.verify_result_json.get(),
        };

        match serde_json::to_string_pretty(&doc) {
            Ok(payload) => {
                match crate::web::storage::export_text_file("tabula-workspace.json", &payload) {
                    Ok(()) => s.set_status_line.set("Workspace exported".to_string()),
                    Err(e) => s.set_status_line.set(format!("Export failed: {e}")),
                }
            }
            Err(e) => s.set_status_line.set(format!("Export failed: {e}")),
        }
    }
}

// ── Export proof ────────────────────────────────────────────────────

pub(crate) fn export_proof(s: AppSignals) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        let data = s.proof_json.get();
        if data.trim().is_empty() {
            s.set_status_line
                .set("No proof artifact to export".to_string());
            return;
        }

        match crate::web::storage::export_text_file("tabula-proof.json", &data) {
            Ok(()) => s.set_status_line.set("Proof artifact exported".to_string()),
            Err(e) => s.set_status_line.set(format!("Proof export failed: {e}")),
        }
    }
}

// ── Import workspace from JSON text ─────────────────────────────────

pub(crate) fn import_workspace_text(
    s: AppSignals,
) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        let payload = s.workspace_import_json.get();
        match serde_json::from_str::<crate::web::models::WorkspaceDoc>(&payload) {
            Ok(ws) => {
                s.set_daemon_url.set(ws.daemon_url);
                s.set_auth_token.set(ws.auth_token);
                s.set_program_source.set(ws.program_source);
                s.set_state_json.set(ws.state_json);
                s.set_batch_json.set(ws.batch_json);
                s.set_include_trace.set(ws.include_trace);
                s.set_proof_json.set(ws.proof_json);
                s.set_verify_result_json.set(ws.verify_result_json);
                s.set_pending_state_after.set(None);
                s.set_last_run_id.set(None);
                s.clear_verify_gate();
                s.set_status_line
                    .set("Workspace imported from JSON text".to_string());
                s.append_history(
                    "workspace_import",
                    true,
                    "imported from textarea".to_string(),
                );
                s.persist();
            }
            Err(e) => {
                s.set_status_line.set(format!("Import failed: {e}"));
                s.append_history("workspace_import", false, e.to_string());
            }
        }
    }
}

// ── File picker triggers ────────────────────────────────────────────

pub(crate) fn open_file_picker(
    input_ref: NodeRef<leptos::html::Input>,
) -> impl Fn(web_sys::MouseEvent) + Clone + 'static {
    move |_| {
        if let Some(el) = input_ref.get() {
            el.click();
        }
    }
}

// ── File change handlers ────────────────────────────────────────────

pub(crate) fn on_proof_file_change(
    s: AppSignals,
    reader_holder: Rc<RefCell<Option<FileReader>>>,
) -> impl Fn(web_sys::Event) + Clone + 'static {
    move |ev: web_sys::Event| {
        let Some(target) = ev.target() else {
            return;
        };
        let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        let gloo_file = File::from(file);
        let task = read_as_text(&gloo_file, move |result| match result {
            Ok(text) => {
                s.set_proof_json.set(text);
                s.set_status_line
                    .set("Proof artifact imported from file".to_string());
                s.persist();
            }
            Err(err) => {
                s.set_status_line
                    .set(format!("Failed to read proof file: {err}"));
            }
        });

        *reader_holder.borrow_mut() = Some(task);
    }
}

pub(crate) fn on_workspace_file_change(
    s: AppSignals,
    reader_holder: Rc<RefCell<Option<FileReader>>>,
) -> impl Fn(web_sys::Event) + Clone + 'static {
    move |ev: web_sys::Event| {
        let Some(target) = ev.target() else {
            return;
        };
        let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        let gloo_file = File::from(file);
        let task = read_as_text(&gloo_file, move |result| match result {
            Ok(text) => {
                s.set_workspace_import_json.set(text);
                s.set_status_line
                    .set("Workspace JSON loaded to import box".to_string());
            }
            Err(err) => {
                s.set_status_line
                    .set(format!("Failed to read workspace file: {err}"));
            }
        });

        *reader_holder.borrow_mut() = Some(task);
    }
}
